//! The `MailBackend` trait and associated value types.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use imt_core::{Flag, FolderRole, MessageBody, MessageHeaders};

use crate::error::Result;

/// Lightweight description of a server folder, returned by `list_folders`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolderInfo {
    /// Server path (e.g. `INBOX`, `[Gmail]/Sent Mail`).
    pub path: String,
    /// Display name (typically the last segment of `path`).
    pub name: String,
    /// Special-use role detected from IMAP attributes.
    pub role: FolderRole,
    /// UIDVALIDITY of the folder; 0 if the server did not advertise it.
    pub uid_validity: u32,
    /// Next UID the server will assign; 0 if unknown.
    pub uid_next: u32,
    /// Total message count.
    pub message_count: u32,
    /// Unread message count.
    pub unread_count: u32,
}

/// State of the currently selected folder, returned by `select_folder`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FolderState {
    /// UIDVALIDITY value as advertised by the server.
    pub uid_validity: u32,
    /// Next UID the server will assign.
    pub uid_next: u32,
    /// Number of messages currently in the folder.
    pub exists: u32,
    /// Number of unseen messages (0 if the server did not report this).
    pub unseen: u32,
}

/// Selector for which UIDs should be fetched.
#[derive(Debug, Clone)]
pub enum UidRange {
    /// Inclusive `[start, end]` UID range.
    Range(u32, u32),
    /// Explicit set of UIDs.
    Set(Vec<u32>),
    /// Every message in the folder.
    All,
}

impl UidRange {
    /// Render this range as an IMAP UID set string (e.g. `1:10`, `1,5,9`, `1:*`).
    pub fn to_imap_set(&self) -> String {
        match self {
            UidRange::Range(start, end) => format!("{}:{}", start, end),
            UidRange::Set(uids) => {
                if uids.is_empty() {
                    String::new()
                } else {
                    uids.iter()
                        .map(|u| u.to_string())
                        .collect::<Vec<_>>()
                        .join(",")
                }
            }
            UidRange::All => "1:*".to_string(),
        }
    }
}

/// Envelope-level fetch result. The caller is responsible for filling in
/// account/folder identifiers when materialising into a `Message`.
#[derive(Debug, Clone)]
pub struct EnvelopeFetch {
    /// Server-assigned UID.
    pub uid: u32,
    /// Parsed RFC 822 headers.
    pub headers: MessageHeaders,
    /// IMAP flags currently set on the message.
    pub flags: Vec<Flag>,
    /// Size in bytes (from `RFC822.SIZE`).
    pub size: u64,
    /// Server-side internal date.
    pub internal_date: DateTime<Utc>,
    /// Short text snippet; empty until full bodies are fetched elsewhere.
    pub snippet: String,
    /// Whether the message appears to carry attachments, detected from the
    /// Content-Type header at envelope time (no body fetch required).
    pub has_attachments: bool,
}

/// Push notification produced while a backend is in IDLE.
#[derive(Debug, Clone)]
pub enum IdleEvent {
    /// New `EXISTS` count reported by the server.
    Exists(u32),
    /// A message sequence number was expunged.
    Expunge(u32),
    /// Flags changed for the given sequence number.
    Flags(u32),
}

/// Handle to an in-progress IDLE session.
pub struct IdleHandle {
    pub(crate) inner: Box<dyn IdleHandleImpl + Send>,
}

impl IdleHandle {
    /// Wait for the next `IdleEvent` from the server.
    pub async fn next(&mut self) -> Result<IdleEvent> {
        self.inner.next_event().await
    }

    /// End the IDLE session and reclaim the underlying connection.
    pub async fn done(self) -> Result<()> {
        self.inner.terminate().await
    }
}

/// Internal trait used to erase the concrete IDLE implementation behind `IdleHandle`.
#[async_trait]
pub trait IdleHandleImpl {
    /// Block until the next push event is available.
    async fn next_event(&mut self) -> Result<IdleEvent>;
    /// End the IDLE command and clean up resources.
    async fn terminate(self: Box<Self>) -> Result<()>;
}

/// Async trait implemented by every protocol adapter (IMAP today, JMAP later).
#[async_trait]
pub trait MailBackend: Send {
    /// Open the connection and authenticate.
    async fn connect(&mut self) -> Result<()>;

    /// Cleanly tear the connection down.
    async fn disconnect(&mut self) -> Result<()>;

    /// Enumerate every folder visible to the authenticated user.
    async fn list_folders(&mut self) -> Result<Vec<FolderInfo>>;

    /// SELECT (or EXAMINE) the given folder, returning its current state.
    async fn select_folder(&mut self, path: &str) -> Result<FolderState>;

    /// Fetch envelope metadata (no body) for the given UID range.
    async fn fetch_envelopes(
        &mut self,
        folder: &str,
        uid_range: UidRange,
    ) -> Result<Vec<EnvelopeFetch>>;

    /// Fetch and parse the full body of a single message.
    async fn fetch_body(&mut self, folder: &str, uid: u32) -> Result<MessageBody>;

    /// Add and remove flags atomically on a single UID.
    async fn set_flags(
        &mut self,
        folder: &str,
        uid: u32,
        add: &[Flag],
        remove: &[Flag],
    ) -> Result<()>;

    /// APPEND a raw RFC 822 message into the given folder.
    async fn append(
        &mut self,
        folder: &str,
        raw_rfc822: &[u8],
        flags: &[Flag],
    ) -> Result<u32>;

    /// Move a message identified by `uid` from `folder` to `dest_folder`.
    /// Uses IMAP MOVE when advertised; otherwise falls back to UID COPY +
    /// STORE +FLAGS \Deleted + EXPUNGE.
    async fn move_uid(
        &mut self,
        folder: &str,
        uid: u32,
        dest_folder: &str,
    ) -> Result<()>;

    /// Hard-delete every message in `folder` by marking all UIDs `\Deleted`
    /// and issuing EXPUNGE. Used for "empty trash".
    async fn expunge_folder(&mut self, folder: &str) -> Result<()>;

    /// Permanently remove a single message (`uid`) from `folder`: mark it
    /// `\Deleted` and expunge just that UID (via UID EXPUNGE when the server
    /// advertises UIDPLUS, otherwise a folder EXPUNGE). Used for the per-account
    /// "do not leave a copy on the server" option after a body is downloaded.
    async fn delete_uid(&mut self, folder: &str, uid: u32) -> Result<()>;

    /// Begin an IDLE session against `folder`. Falls back to polling if the
    /// server does not advertise IDLE.
    async fn idle(&mut self, folder: &str) -> Result<IdleHandle>;
}
