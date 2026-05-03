//! imt-store: SQLite persistence layer for Indicium Mail TUI.

pub mod db;
pub mod repo;
pub mod secrets;

pub use db::Db;
pub use repo::accounts::AccountRepo;
pub use repo::drafts::DraftRepo;
pub use repo::folders::FolderRepo;
pub use repo::messages::MessageRepo;
pub use repo::search::SearchRepo;

use thiserror::Error;

/// Errors produced by the store layer.
#[derive(Debug, Error)]
pub enum StoreError {
    /// Underlying sqlx error.
    #[error("sqlx: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// Migration error.
    #[error("migrate: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// JSON (de)serialization error.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// IO error.
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// Row not found.
    #[error("not found")]
    NotFound,
    /// Other error.
    #[error("{0}")]
    Other(String),
}

/// Result alias used by the store layer.
pub type Result<T> = std::result::Result<T, StoreError>;
