pub mod protocol;
pub mod server;
pub mod transport;
pub mod tools;

use std::sync::Arc;

/// Shared state passed to every tool handler.
pub struct McpContext {
    pub db: Arc<imt_store::Db>,
    pub engine: Arc<imt_sync::SyncEngine>,
}

pub use server::run;
