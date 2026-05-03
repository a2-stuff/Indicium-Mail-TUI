//! Account persistence.

use imt_core::{Account, AccountId, Address, ImapConfig, SmtpConfig};
use sqlx::Row;
use sqlx::SqlitePool;

use crate::repo::{uuid_bytes, uuid_from_slice};
use crate::{Result, StoreError};

/// CRUD operations for accounts.
pub struct AccountRepo<'a>(pub &'a SqlitePool);

impl<'a> AccountRepo<'a> {
    /// Wrap a pool reference.
    pub fn new(pool: &'a SqlitePool) -> Self {
        Self(pool)
    }

    /// Insert or update an account row.
    pub async fn upsert(&self, a: &Account) -> Result<()> {
        let imap_json = serde_json::to_string(&a.imap)?;
        let smtp_json = serde_json::to_string(&a.smtp)?;
        let id_bytes = uuid_bytes(&a.id.0);
        sqlx::query(
            "INSERT INTO accounts (id, display_name, address_name, address_email, imap_json, smtp_json, ord) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(id) DO UPDATE SET \
                display_name = excluded.display_name, \
                address_name = excluded.address_name, \
                address_email = excluded.address_email, \
                imap_json = excluded.imap_json, \
                smtp_json = excluded.smtp_json, \
                ord = excluded.ord",
        )
        .bind(&id_bytes)
        .bind(&a.display_name)
        .bind(&a.address.name)
        .bind(&a.address.email)
        .bind(&imap_json)
        .bind(&smtp_json)
        .bind(a.order)
        .execute(self.0)
        .await?;
        Ok(())
    }

    /// Fetch an account by id.
    pub async fn get(&self, id: AccountId) -> Result<Account> {
        let id_bytes = uuid_bytes(&id.0);
        let row = sqlx::query(
            "SELECT id, display_name, address_name, address_email, imap_json, smtp_json, ord \
             FROM accounts WHERE id = ?1",
        )
        .bind(&id_bytes)
        .fetch_optional(self.0)
        .await?
        .ok_or(StoreError::NotFound)?;
        row_to_account(&row)
    }

    /// List all accounts ordered by `ord`.
    pub async fn list(&self) -> Result<Vec<Account>> {
        let rows = sqlx::query(
            "SELECT id, display_name, address_name, address_email, imap_json, smtp_json, ord \
             FROM accounts ORDER BY ord ASC, display_name ASC",
        )
        .fetch_all(self.0)
        .await?;
        rows.iter().map(row_to_account).collect()
    }

    /// Delete an account by id.
    pub async fn delete(&self, id: AccountId) -> Result<()> {
        let id_bytes = uuid_bytes(&id.0);
        sqlx::query("DELETE FROM accounts WHERE id = ?1")
            .bind(&id_bytes)
            .execute(self.0)
            .await?;
        Ok(())
    }
}

fn row_to_account(row: &sqlx::sqlite::SqliteRow) -> Result<Account> {
    let id_bytes: Vec<u8> = row.try_get("id")?;
    let id_uuid = uuid_from_slice(&id_bytes).map_err(|e| StoreError::Other(e.to_string()))?;
    let display_name: String = row.try_get("display_name")?;
    let address_name: Option<String> = row.try_get("address_name")?;
    let address_email: String = row.try_get("address_email")?;
    let imap_json: String = row.try_get("imap_json")?;
    let smtp_json: String = row.try_get("smtp_json")?;
    let ord: i32 = row.try_get("ord")?;
    let imap: ImapConfig = serde_json::from_str(&imap_json)?;
    let smtp: SmtpConfig = serde_json::from_str(&smtp_json)?;
    Ok(Account {
        id: AccountId(id_uuid),
        display_name,
        address: Address {
            name: address_name,
            email: address_email,
        },
        imap,
        smtp,
        order: ord,
    })
}
