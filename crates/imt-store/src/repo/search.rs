//! FTS5-backed message search.

use imt_core::{AccountId, MessageId};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::repo::{uuid_bytes, uuid_from_slice};
use crate::{Result, StoreError};

/// Read-only search queries.
pub struct SearchRepo<'a>(pub &'a SqlitePool);

impl<'a> SearchRepo<'a> {
    /// Wrap a pool reference.
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self(pool)
    }

    /// Run an FTS5 MATCH query, optionally constrained to a single account.
    pub async fn query(
        &self,
        account_id: Option<AccountId>,
        query: &str,
        limit: u32,
    ) -> Result<Vec<MessageId>> {
        let escaped = escape_fts_query(query);
        let rows = if let Some(acc) = account_id {
            let acc_bytes = uuid_bytes(&acc.0);
            sqlx::query(
                "SELECT m.id AS id FROM messages m \
                 JOIN messages_fts f ON f.rowid = m.rowid \
                 WHERE messages_fts MATCH ?1 AND m.account_id = ?2 \
                 ORDER BY m.internal_date DESC LIMIT ?3",
            )
            .bind(&escaped)
            .bind(&acc_bytes)
            .bind(limit as i64)
            .fetch_all(self.0)
            .await?
        } else {
            sqlx::query(
                "SELECT m.id AS id FROM messages m \
                 JOIN messages_fts f ON f.rowid = m.rowid \
                 WHERE messages_fts MATCH ?1 \
                 ORDER BY m.internal_date DESC LIMIT ?2",
            )
            .bind(&escaped)
            .bind(limit as i64)
            .fetch_all(self.0)
            .await?
        };
        let mut ids = Vec::with_capacity(rows.len());
        for row in rows {
            let bytes: Vec<u8> = row.try_get("id")?;
            let id =
                uuid_from_slice(&bytes).map_err(|e| StoreError::Other(e.to_string()))?;
            ids.push(MessageId(id));
        }
        Ok(ids)
    }
}

/// Quote each whitespace-split token to defang FTS5 syntax in user input.
fn escape_fts_query(input: &str) -> String {
    let mut parts = Vec::new();
    for tok in input.split_whitespace() {
        let cleaned: String = tok.replace('"', "");
        if cleaned.is_empty() {
            continue;
        }
        parts.push(format!("\"{}\"", cleaned));
    }
    if parts.is_empty() {
        "\"\"".to_string()
    } else {
        parts.join(" ")
    }
}
