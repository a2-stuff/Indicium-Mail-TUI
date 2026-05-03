use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AccountId, FolderId, MessageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub Uuid);

impl ThreadId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for ThreadId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thread {
    pub id: ThreadId,
    pub account_id: AccountId,
    pub folder_id: FolderId,
    pub subject: String,
    pub message_ids: Vec<MessageId>,
    pub unread_count: u32,
    pub last_activity: DateTime<Utc>,
}
