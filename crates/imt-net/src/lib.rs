//! imt-net: protocol adapters (IMAP, POP3, SMTP) implementing MailBackend.

pub mod backend;
pub mod error;
pub mod imap;
pub mod oauth;
pub mod pop3;
pub mod smtp;

pub use backend::{
    EnvelopeFetch, FolderInfo, FolderState, IdleEvent, IdleHandle, MailBackend, UidRange,
};
pub use error::{NetError, Result};
pub use imap::ImapBackend;
pub use pop3::Pop3Backend;
pub use oauth::{xoauth2_sasl, CsrfToken, OAuthFlow, OAuthProvider, OAuthTokens, PkceVerifier};
pub use smtp::{build_rfc822, BuildDraft, DraftAttachmentRef, SmtpSender};
