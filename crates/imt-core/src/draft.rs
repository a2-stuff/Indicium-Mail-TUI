use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AccountId, Address, MessageId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DraftId(pub Uuid);

impl DraftId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for DraftId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DraftKind {
    New,
    Reply,
    ReplyAll,
    Forward,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftAttachment {
    pub filename: String,
    pub mime_type: String,
    pub path: std::path::PathBuf,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    pub id: DraftId,
    pub account_id: AccountId,
    pub kind: DraftKind,
    /// Original message being replied to / forwarded.
    pub in_reply_to: Option<MessageId>,
    pub from: Address,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub subject: String,
    pub body_text: String,
    pub attachments: Vec<DraftAttachment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
