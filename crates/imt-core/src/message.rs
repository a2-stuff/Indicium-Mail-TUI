use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AccountId, Address, Flag, FolderId, ThreadId, Uid};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct MessageId(pub Uuid);

impl MessageId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for MessageId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageHeaders {
    /// RFC 822 Message-ID, e.g. `<abc@example.com>`. Used for threading.
    pub rfc_message_id: Option<String>,
    pub in_reply_to: Option<String>,
    pub references: Vec<String>,
    pub from: Vec<Address>,
    pub to: Vec<Address>,
    pub cc: Vec<Address>,
    pub bcc: Vec<Address>,
    pub reply_to: Vec<Address>,
    pub subject: String,
    pub date: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub mime_type: String,
    pub size: u64,
    /// Logical part identifier within the message (e.g. IMAP section).
    pub part_id: String,
    pub content_id: Option<String>,
    pub inline: bool,
    /// Path to a temp file holding the decoded bytes. Set after body fetch; not persisted to DB.
    #[serde(skip)]
    pub temp_path: Option<std::path::PathBuf>,
}

/// Decoded body bundles.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MessageBody {
    pub text_plain: Option<String>,
    pub text_html: Option<String>,
    pub attachments: Vec<Attachment>,
}

/// One MIME part as stored.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessagePart {
    pub part_id: String,
    pub mime_type: String,
    pub charset: Option<String>,
    pub filename: Option<String>,
    pub size: u64,
    pub inline: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: MessageId,
    pub account_id: AccountId,
    pub folder_id: FolderId,
    pub thread_id: Option<ThreadId>,
    pub uid: Uid,
    pub headers: MessageHeaders,
    pub flags: Vec<Flag>,
    pub size: u64,
    /// Lazy: full body is fetched on demand. None means "envelope only".
    pub body: Option<MessageBody>,
    /// Whether the message carries attachments. Set from BODYSTRUCTURE / the
    /// Content-Type header at envelope-sync time (so the list can show it
    /// without fetching the body), and corrected to the exact value once the
    /// full body is fetched.
    #[serde(default)]
    pub has_attachments: bool,
    /// Short preview (first ~256 chars of plain text).
    pub snippet: String,
    pub internal_date: DateTime<Utc>,
}

impl Message {
    pub fn is_unread(&self) -> bool {
        !self.flags.contains(&Flag::Seen)
    }
    pub fn is_flagged(&self) -> bool {
        self.flags.contains(&Flag::Flagged)
    }
    pub fn is_answered(&self) -> bool {
        self.flags.contains(&Flag::Answered)
    }
}
