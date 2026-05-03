//! imt-core: shared domain types for Indicium Mail TUI.
//!
//! Pure data types only. No I/O, no async. Every other crate depends on this.

pub mod address;
pub mod account;
pub mod folder;
pub mod message;
pub mod thread;
pub mod flag;
pub mod draft;
pub mod event;
pub mod error;
pub mod id;

pub use address::Address;
pub use account::{Account, AccountId, AuthMethod, ImapConfig, NewAccountForm, OAuthProvider, SmtpConfig, Tls};
pub use folder::{Folder, FolderId, FolderRole};
pub use message::{Message, MessageId, MessageHeaders, MessagePart, MessageBody, Attachment};
pub use thread::{Thread, ThreadId};
pub use flag::Flag;
pub use draft::{Draft, DraftId};
pub use event::SyncEvent;
pub use error::{CoreError, Result};
pub use id::Uid;
