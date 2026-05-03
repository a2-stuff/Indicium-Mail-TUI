//! Draft persistence.

use chrono::{DateTime, Utc};
use imt_core::{
    draft::{DraftAttachment, DraftKind},
    AccountId, Address, Draft, DraftId, MessageId,
};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::repo::{uuid_bytes, uuid_from_slice};
use crate::{Result, StoreError};

/// CRUD operations for drafts.
pub struct DraftRepo<'a>(pub &'a SqlitePool);

impl<'a> DraftRepo<'a> {
    /// Wrap a pool reference.
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self(pool)
    }

    /// Insert or update a draft.
    pub async fn upsert(&self, d: &Draft) -> Result<()> {
        let id_bytes = uuid_bytes(&d.id.0);
        let acc_bytes = uuid_bytes(&d.account_id.0);
        let in_reply_bytes: Option<Vec<u8>> = d.in_reply_to.map(|m| uuid_bytes(&m.0));
        let kind = kind_to_str(d.kind);
        let from_json = serde_json::to_string(&d.from)?;
        let to_json = serde_json::to_string(&d.to)?;
        let cc_json = serde_json::to_string(&d.cc)?;
        let bcc_json = serde_json::to_string(&d.bcc)?;
        let attachments_json = serde_json::to_string(&d.attachments)?;
        sqlx::query(
            "INSERT INTO drafts (id, account_id, kind, in_reply_to, from_json, to_json, cc_json, bcc_json, subject, body_text, attachments_json, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13) \
             ON CONFLICT(id) DO UPDATE SET \
                kind = excluded.kind, \
                in_reply_to = excluded.in_reply_to, \
                from_json = excluded.from_json, \
                to_json = excluded.to_json, \
                cc_json = excluded.cc_json, \
                bcc_json = excluded.bcc_json, \
                subject = excluded.subject, \
                body_text = excluded.body_text, \
                attachments_json = excluded.attachments_json, \
                updated_at = excluded.updated_at",
        )
        .bind(&id_bytes)
        .bind(&acc_bytes)
        .bind(kind)
        .bind(in_reply_bytes.as_deref())
        .bind(&from_json)
        .bind(&to_json)
        .bind(&cc_json)
        .bind(&bcc_json)
        .bind(&d.subject)
        .bind(&d.body_text)
        .bind(&attachments_json)
        .bind(d.created_at)
        .bind(d.updated_at)
        .execute(self.0)
        .await?;
        Ok(())
    }

    /// Fetch a draft by id.
    pub async fn get(&self, id: DraftId) -> Result<Draft> {
        let id_bytes = uuid_bytes(&id.0);
        let row = sqlx::query(SELECT_DRAFT)
            .bind(&id_bytes)
            .fetch_optional(self.0)
            .await?
            .ok_or(StoreError::NotFound)?;
        row_to_draft(&row)
    }

    /// List drafts for an account, most recently updated first.
    pub async fn list_by_account(&self, account_id: AccountId) -> Result<Vec<Draft>> {
        let acc_bytes = uuid_bytes(&account_id.0);
        let rows = sqlx::query(LIST_DRAFTS)
            .bind(&acc_bytes)
            .fetch_all(self.0)
            .await?;
        rows.iter().map(row_to_draft).collect()
    }

    /// Delete a draft.
    pub async fn delete(&self, id: DraftId) -> Result<()> {
        let id_bytes = uuid_bytes(&id.0);
        sqlx::query("DELETE FROM drafts WHERE id = ?1")
            .bind(&id_bytes)
            .execute(self.0)
            .await?;
        Ok(())
    }
}

const SELECT_DRAFT: &str = "SELECT id, account_id, kind, in_reply_to, from_json, to_json, cc_json, bcc_json, subject, body_text, attachments_json, created_at, updated_at FROM drafts WHERE id = ?1";

const LIST_DRAFTS: &str = "SELECT id, account_id, kind, in_reply_to, from_json, to_json, cc_json, bcc_json, subject, body_text, attachments_json, created_at, updated_at FROM drafts WHERE account_id = ?1 ORDER BY updated_at DESC";

fn kind_to_str(k: DraftKind) -> &'static str {
    match k {
        DraftKind::New => "new",
        DraftKind::Reply => "reply",
        DraftKind::ReplyAll => "reply_all",
        DraftKind::Forward => "forward",
    }
}

fn kind_from_str(s: &str) -> DraftKind {
    match s {
        "reply" => DraftKind::Reply,
        "reply_all" => DraftKind::ReplyAll,
        "forward" => DraftKind::Forward,
        _ => DraftKind::New,
    }
}

fn row_to_draft(row: &sqlx::sqlite::SqliteRow) -> Result<Draft> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let acc_bytes: Vec<u8> = row.try_get("account_id")?;
    let kind: String = row.try_get("kind")?;
    let in_reply_bytes: Option<Vec<u8>> = row.try_get("in_reply_to")?;
    let from_json: String = row.try_get("from_json")?;
    let to_json: String = row.try_get("to_json")?;
    let cc_json: String = row.try_get("cc_json")?;
    let bcc_json: String = row.try_get("bcc_json")?;
    let subject: String = row.try_get("subject")?;
    let body_text: String = row.try_get("body_text")?;
    let attachments_json: String = row.try_get("attachments_json")?;
    let created_at: DateTime<Utc> = row.try_get("created_at")?;
    let updated_at: DateTime<Utc> = row.try_get("updated_at")?;

    let id_uuid = uuid_from_slice(&id_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let acc_uuid = uuid_from_slice(&acc_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let in_reply_to = match in_reply_bytes {
        Some(b) => Some(MessageId(
            uuid_from_slice(&b).map_err(|e| StoreError::Other(e.to_string()))?,
        )),
        None => None,
    };
    let from: Address = serde_json::from_str(&from_json)?;
    let to: Vec<Address> = serde_json::from_str(&to_json)?;
    let cc: Vec<Address> = serde_json::from_str(&cc_json)?;
    let bcc: Vec<Address> = serde_json::from_str(&bcc_json)?;
    let attachments: Vec<DraftAttachment> = serde_json::from_str(&attachments_json)?;

    Ok(Draft {
        id: DraftId(id_uuid),
        account_id: AccountId(acc_uuid),
        kind: kind_from_str(&kind),
        in_reply_to,
        from,
        to,
        cc,
        bcc,
        subject,
        body_text,
        attachments,
        created_at,
        updated_at,
    })
}
