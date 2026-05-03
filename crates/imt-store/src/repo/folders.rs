//! Folder persistence.

use imt_core::{AccountId, Folder, FolderId, FolderRole};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::repo::{uuid_bytes, uuid_from_slice};
use crate::{Result, StoreError};

/// CRUD operations for folders.
pub struct FolderRepo<'a>(pub &'a SqlitePool);

impl<'a> FolderRepo<'a> {
    /// Wrap a pool reference.
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self(pool)
    }

    /// Insert or update a folder row.
    pub async fn upsert(&self, f: &Folder) -> Result<()> {
        let id_bytes = uuid_bytes(&f.id.0);
        let acc_bytes = uuid_bytes(&f.account_id.0);
        let role = role_to_str(f.role);
        sqlx::query(
            "INSERT INTO folders (id, account_id, path, name, role, uid_validity, uid_next, message_count, unread_count) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
             ON CONFLICT(account_id, path) DO UPDATE SET \
                name = excluded.name, \
                role = excluded.role, \
                uid_validity = excluded.uid_validity, \
                uid_next = excluded.uid_next, \
                message_count = excluded.message_count, \
                unread_count = excluded.unread_count",
        )
        .bind(&id_bytes)
        .bind(&acc_bytes)
        .bind(&f.path)
        .bind(&f.name)
        .bind(role)
        .bind(f.uid_validity as i64)
        .bind(f.uid_next as i64)
        .bind(f.message_count as i64)
        .bind(f.unread_count as i64)
        .execute(self.0)
        .await?;
        Ok(())
    }

    /// Fetch a folder by id.
    pub async fn get(&self, id: FolderId) -> Result<Folder> {
        let id_bytes = uuid_bytes(&id.0);
        let row = sqlx::query(
            "SELECT id, account_id, path, name, role, uid_validity, uid_next, message_count, unread_count \
             FROM folders WHERE id = ?1",
        )
        .bind(&id_bytes)
        .fetch_optional(self.0)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_folder(&row)
    }

    /// List folders for an account.
    pub async fn list_by_account(&self, account_id: AccountId) -> Result<Vec<Folder>> {
        let acc_bytes = uuid_bytes(&account_id.0);
        let rows = sqlx::query(
            "SELECT id, account_id, path, name, role, uid_validity, uid_next, message_count, unread_count \
             FROM folders WHERE account_id = ?1 ORDER BY path ASC",
        )
        .bind(&acc_bytes)
        .fetch_all(self.0)
        .await?;
        rows.iter().map(row_to_folder).collect()
    }

    /// Update message and unread counts for a folder.
    pub async fn update_counts(&self, id: FolderId, message_count: u32, unread_count: u32) -> Result<()> {
        let id_bytes = uuid_bytes(&id.0);
        sqlx::query("UPDATE folders SET message_count = ?1, unread_count = ?2 WHERE id = ?3")
            .bind(message_count as i64)
            .bind(unread_count as i64)
            .bind(&id_bytes)
            .execute(self.0)
            .await?;
        Ok(())
    }

    /// Delete a folder by id.
    pub async fn delete(&self, id: FolderId) -> Result<()> {
        let id_bytes = uuid_bytes(&id.0);
        sqlx::query("DELETE FROM folders WHERE id = ?1")
            .bind(&id_bytes)
            .execute(self.0)
            .await?;
        Ok(())
    }
}

fn role_to_str(r: FolderRole) -> &'static str {
    match r {
        FolderRole::Inbox => "inbox",
        FolderRole::Sent => "sent",
        FolderRole::Drafts => "drafts",
        FolderRole::Trash => "trash",
        FolderRole::Junk => "junk",
        FolderRole::Archive => "archive",
        FolderRole::Other => "other",
    }
}

fn role_from_str(s: &str) -> FolderRole {
    match s {
        "inbox" => FolderRole::Inbox,
        "sent" => FolderRole::Sent,
        "drafts" => FolderRole::Drafts,
        "trash" => FolderRole::Trash,
        "junk" => FolderRole::Junk,
        "archive" => FolderRole::Archive,
        _ => FolderRole::Other,
    }
}

fn row_to_folder(row: &sqlx::sqlite::SqliteRow) -> Result<Folder> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let acc_bytes: Vec<u8> = row.try_get("account_id")?;
    let id_uuid = uuid_from_slice(&id_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let acc_uuid = uuid_from_slice(&acc_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let path: String = row.try_get("path")?;
    let name: String = row.try_get("name")?;
    let role: String = row.try_get("role")?;
    let uid_validity: i64 = row.try_get("uid_validity")?;
    let uid_next: i64 = row.try_get("uid_next")?;
    let message_count: i64 = row.try_get("message_count")?;
    let unread_count: i64 = row.try_get("unread_count")?;
    Ok(Folder {
        id: FolderId(id_uuid),
        account_id: AccountId(acc_uuid),
        path,
        name,
        role: role_from_str(&role),
        uid_validity: uid_validity as u32,
        uid_next: uid_next as u32,
        message_count: message_count as u32,
        unread_count: unread_count as u32,
    })
}
