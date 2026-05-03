//! imt-sync: bridges `imt-net` (protocol I/O) and `imt-store` (persistence),
//! emitting `SyncEvent`s for the TUI to consume.

pub mod account_task;
pub mod engine;
pub mod error;
pub mod password;
pub mod snippet;

pub use engine::SyncEngine;
pub use error::{Result, SyncError};
pub use snippet::make_snippet;
