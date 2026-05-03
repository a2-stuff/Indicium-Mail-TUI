use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::AccountId;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FolderId(pub Uuid);

impl FolderId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for FolderId {
    fn default() -> Self { Self::new() }
}

/// Special-use semantic role of a folder (RFC 6154).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FolderRole {
    Inbox,
    Sent,
    Drafts,
    Trash,
    Junk,
    Archive,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Folder {
    pub id: FolderId,
    pub account_id: AccountId,
    /// Server path, e.g. `INBOX`, `[Gmail]/Sent Mail`.
    pub path: String,
    /// Display name (last path segment).
    pub name: String,
    pub role: FolderRole,
    pub uid_validity: u32,
    pub uid_next: u32,
    pub message_count: u32,
    pub unread_count: u32,
}
