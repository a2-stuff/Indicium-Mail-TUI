//! Database handle and migration runner.

use std::path::Path;
use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::Result;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Owned handle around a sqlite connection pool.
#[derive(Debug, Clone)]
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (or create) the database at `path` and run all pending migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let path_str = path.to_string_lossy();
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path_str))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(std::time::Duration::from_millis(5000));
        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(opts)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// Borrow the inner connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}
