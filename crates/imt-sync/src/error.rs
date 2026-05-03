//! Error type for the sync engine.

use thiserror::Error;

/// Errors produced by the sync engine.
#[derive(Debug, Error)]
pub enum SyncError {
    /// Underlying store failure.
    #[error("store: {0}")]
    Store(#[from] imt_store::StoreError),
    /// Underlying network failure.
    #[error("net: {0}")]
    Net(#[from] imt_net::NetError),
    /// Operation cancelled (e.g. shutdown).
    #[error("cancelled")]
    Cancelled,
    /// Catch-all.
    #[error("{0}")]
    Other(String),
}

/// Result alias used throughout `imt-sync`.
pub type Result<T> = std::result::Result<T, SyncError>;
