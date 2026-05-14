//! Message persistence (envelopes, bodies, flags).

use chrono::{DateTime, Utc};
use imt_core::{
    AccountId, Address, Attachment, Flag, FolderId, Message, MessageBody, MessageHeaders,
    MessageId, ThreadId, Uid,
};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::repo::{uuid_bytes, uuid_from_slice};
use crate::{Result, StoreError};

/// CRUD operations for messages.
pub struct MessageRepo<'a>(pub &'a SqlitePool);

impl<'a> MessageRepo<'a> {
    /// Wrap a pool reference.
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self(pool)
    }

    /// Insert or replace the envelope of a message (no body).
    pub async fn upsert_envelope(&self, m: &Message) -> Result<()> {
        let id_bytes = uuid_bytes(&m.id.0);
        let acc_bytes = uuid_bytes(&m.account_id.0);
        let folder_bytes = uuid_bytes(&m.folder_id.0);
        let thread_bytes: Option<Vec<u8>> = m.thread_id.map(|t| uuid_bytes(&t.0));
        let refs_json = serde_json::to_string(&m.headers.references)?;
        let from_json = serde_json::to_string(&m.headers.from)?;
        let to_json = serde_json::to_string(&m.headers.to)?;
        let cc_json = serde_json::to_string(&m.headers.cc)?;
        let bcc_json = serde_json::to_string(&m.headers.bcc)?;
        let reply_to_json = serde_json::to_string(&m.headers.reply_to)?;
        let flags_json = serde_json::to_string(&m.flags)?;
        let has_body: i64 = if m.body.is_some() { 1 } else { 0 };
        let body_text: Option<String> = m.body.as_ref().and_then(|b| b.text_plain.clone());
        let body_html: Option<String> = m.body.as_ref().and_then(|b| b.text_html.clone());
        let attachments_json = serde_json::to_string(
            &m.body.as_ref().map(|b| b.attachments.clone()).unwrap_or_default(),
        )?;

        sqlx::query(
            "INSERT INTO messages (\
                id, account_id, folder_id, thread_id, uid, \
                rfc_message_id, in_reply_to, references_json, \
                from_json, to_json, cc_json, bcc_json, reply_to_json, \
                subject, date, internal_date, flags_json, size, snippet, \
                body_text, body_html, attachments_json, has_body) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23) \
             ON CONFLICT(folder_id, uid) DO UPDATE SET \
                id = excluded.id, \
                thread_id = excluded.thread_id, \
                rfc_message_id = excluded.rfc_message_id, \
                in_reply_to = excluded.in_reply_to, \
                references_json = excluded.references_json, \
                from_json = excluded.from_json, \
                to_json = excluded.to_json, \
                cc_json = excluded.cc_json, \
                bcc_json = excluded.bcc_json, \
                reply_to_json = excluded.reply_to_json, \
                subject = excluded.subject, \
                date = excluded.date, \
                internal_date = excluded.internal_date, \
                flags_json = excluded.flags_json, \
                size = excluded.size, \
                snippet = excluded.snippet",
        )
        .bind(&id_bytes)
        .bind(&acc_bytes)
        .bind(&folder_bytes)
        .bind(thread_bytes.as_deref())
        .bind(m.uid.0 as i64)
        .bind(&m.headers.rfc_message_id)
        .bind(&m.headers.in_reply_to)
        .bind(&refs_json)
        .bind(&from_json)
        .bind(&to_json)
        .bind(&cc_json)
        .bind(&bcc_json)
        .bind(&reply_to_json)
        .bind(&m.headers.subject)
        .bind(m.headers.date)
        .bind(m.internal_date)
        .bind(&flags_json)
        .bind(m.size as i64)
        .bind(&m.snippet)
        .bind(&body_text)
        .bind(&body_html)
        .bind(&attachments_json)
        .bind(has_body)
        .execute(self.0)
        .await?;
        Ok(())
    }

    /// Persist the decoded body for a message.
    pub async fn set_body(&self, id: MessageId, body: &MessageBody) -> Result<()> {
        let id_bytes = uuid_bytes(&id.0);
        let attachments_json = serde_json::to_string(&body.attachments)?;
        sqlx::query(
            "UPDATE messages SET body_text = ?1, body_html = ?2, attachments_json = ?3, has_body = 1 \
             WHERE id = ?4",
        )
        .bind(&body.text_plain)
        .bind(&body.text_html)
        .bind(&attachments_json)
        .bind(&id_bytes)
        .execute(self.0)
        .await?;
        Ok(())
    }

    /// Fetch a message by id (including body if stored).
    pub async fn get(&self, id: MessageId) -> Result<Message> {
        let id_bytes = uuid_bytes(&id.0);
        let row = sqlx::query(SELECT_MESSAGE)
            .bind(&id_bytes)
            .fetch_optional(self.0)
            .await?
            .ok_or(StoreError::NotFound)?;
        row_to_message(&row)
    }

    /// Fetch a message by `(folder, uid)` pair.
    pub async fn get_by_uid(&self, folder_id: FolderId, uid: Uid) -> Result<Message> {
        let folder_bytes = uuid_bytes(&folder_id.0);
        let row = sqlx::query(SELECT_MESSAGE_BY_UID)
            .bind(&folder_bytes)
            .bind(uid.0 as i64)
            .fetch_optional(self.0)
            .await?
            .ok_or(StoreError::NotFound)?;
        row_to_message(&row)
    }

    /// List messages in a folder, newest first, paged.
    pub async fn list_by_folder(
        &self,
        folder_id: FolderId,
        limit: u32,
        offset: u32,
    ) -> Result<Vec<Message>> {
        let folder_bytes = uuid_bytes(&folder_id.0);
        let rows = sqlx::query(LIST_BY_FOLDER)
            .bind(&folder_bytes)
            .bind(limit as i64)
            .bind(offset as i64)
            .fetch_all(self.0)
            .await?;
        rows.iter().map(row_to_message).collect()
    }

    /// Replace the flag set for a message.
    pub async fn update_flags(&self, id: MessageId, flags: &[Flag]) -> Result<()> {
        let id_bytes = uuid_bytes(&id.0);
        let flags_json = serde_json::to_string(flags)?;
        sqlx::query("UPDATE messages SET flags_json = ?1 WHERE id = ?2")
            .bind(&flags_json)
            .bind(&id_bytes)
            .execute(self.0)
            .await?;
        Ok(())
    }

    /// Delete every message stored locally for a folder.
    pub async fn delete_by_folder(&self, folder_id: FolderId) -> Result<()> {
        let folder_bytes = uuid_bytes(&folder_id.0);
        sqlx::query("DELETE FROM messages WHERE folder_id = ?1")
            .bind(&folder_bytes)
            .execute(self.0)
            .await?;
        Ok(())
    }

    /// Delete a message identified by `(folder, uid)`.
    pub async fn delete_by_uid(&self, folder_id: FolderId, uid: Uid) -> Result<()> {
        let folder_bytes = uuid_bytes(&folder_id.0);
        sqlx::query("DELETE FROM messages WHERE folder_id = ?1 AND uid = ?2")
            .bind(&folder_bytes)
            .bind(uid.0 as i64)
            .execute(self.0)
            .await?;
        Ok(())
    }
}

const COLUMNS: &str = "id, account_id, folder_id, thread_id, uid, \
    rfc_message_id, in_reply_to, references_json, \
    from_json, to_json, cc_json, bcc_json, reply_to_json, \
    subject, date, internal_date, flags_json, size, snippet, \
    body_text, body_html, attachments_json, has_body";

const SELECT_MESSAGE: &str =
    "SELECT id, account_id, folder_id, thread_id, uid, \
     rfc_message_id, in_reply_to, references_json, \
     from_json, to_json, cc_json, bcc_json, reply_to_json, \
     subject, date, internal_date, flags_json, size, snippet, \
     body_text, body_html, attachments_json, has_body \
     FROM messages WHERE id = ?1";

const SELECT_MESSAGE_BY_UID: &str =
    "SELECT id, account_id, folder_id, thread_id, uid, \
     rfc_message_id, in_reply_to, references_json, \
     from_json, to_json, cc_json, bcc_json, reply_to_json, \
     subject, date, internal_date, flags_json, size, snippet, \
     body_text, body_html, attachments_json, has_body \
     FROM messages WHERE folder_id = ?1 AND uid = ?2";

const LIST_BY_FOLDER: &str =
    "SELECT id, account_id, folder_id, thread_id, uid, \
     rfc_message_id, in_reply_to, references_json, \
     from_json, to_json, cc_json, bcc_json, reply_to_json, \
     subject, date, internal_date, flags_json, size, snippet, \
     body_text, body_html, attachments_json, has_body \
     FROM messages WHERE folder_id = ?1 \
     ORDER BY internal_date DESC LIMIT ?2 OFFSET ?3";

#[allow(dead_code)]
const _COLUMNS_REF: &str = COLUMNS;

fn row_to_message(row: &sqlx::sqlite::SqliteRow) -> Result<Message> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let acc_bytes: Vec<u8> = row.try_get("account_id")?;
    let folder_bytes: Vec<u8> = row.try_get("folder_id")?;
    let thread_bytes: Option<Vec<u8>> = row.try_get("thread_id")?;
    let uid: i64 = row.try_get("uid")?;
    let rfc_message_id: Option<String> = row.try_get("rfc_message_id")?;
    let in_reply_to: Option<String> = row.try_get("in_reply_to")?;
    let references_json: String = row.try_get("references_json")?;
    let from_json: String = row.try_get("from_json")?;
    let to_json: String = row.try_get("to_json")?;
    let cc_json: String = row.try_get("cc_json")?;
    let bcc_json: String = row.try_get("bcc_json")?;
    let reply_to_json: String = row.try_get("reply_to_json")?;
    let subject: String = row.try_get("subject")?;
    let date: DateTime<Utc> = row.try_get("date")?;
    let internal_date: DateTime<Utc> = row.try_get("internal_date")?;
    let flags_json: String = row.try_get("flags_json")?;
    let size: i64 = row.try_get("size")?;
    let snippet: String = row.try_get("snippet")?;
    let body_text: Option<String> = row.try_get("body_text")?;
    let body_html: Option<String> = row.try_get("body_html")?;
    let attachments_json: String = row.try_get("attachments_json")?;
    let has_body: i64 = row.try_get("has_body")?;

    let id_uuid = uuid_from_slice(&id_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let acc_uuid = uuid_from_slice(&acc_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let folder_uuid =
        uuid_from_slice(&folder_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let thread_id = match thread_bytes {
        Some(b) => Some(ThreadId(
            uuid_from_slice(&b).map_err(|e| StoreError::Other(e.to_string()))?,
        )),
        None => None,
    };

    let references: Vec<String> = serde_json::from_str(&references_json)?;
    let from: Vec<Address> = serde_json::from_str(&from_json)?;
    let to: Vec<Address> = serde_json::from_str(&to_json)?;
    let cc: Vec<Address> = serde_json::from_str(&cc_json)?;
    let bcc: Vec<Address> = serde_json::from_str(&bcc_json)?;
    let reply_to: Vec<Address> = serde_json::from_str(&reply_to_json)?;
    let flags: Vec<Flag> = serde_json::from_str(&flags_json)?;
    let attachments: Vec<Attachment> = serde_json::from_str(&attachments_json)?;

    let body = if has_body != 0 {
        Some(MessageBody {
            text_plain: body_text,
            text_html: body_html,
            attachments,
        })
    } else {
        None
    };

    Ok(Message {
        id: MessageId(id_uuid),
        account_id: AccountId(acc_uuid),
        folder_id: FolderId(folder_uuid),
        thread_id,
        uid: Uid(uid as u32),
        headers: MessageHeaders {
            rfc_message_id,
            in_reply_to,
            references,
            from,
            to,
            cc,
            bcc,
            reply_to,
            subject,
            date,
        },
        flags,
        size: size as u64,
        body,
        snippet,
        internal_date,
    })
}
