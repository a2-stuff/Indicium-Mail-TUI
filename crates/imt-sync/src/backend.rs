//! Backend selection: build the right [`MailBackend`] for an account's
//! configured incoming protocol (IMAP or POP3).

use imt_core::{Account, IncomingProtocol};
use imt_net::backend::MailBackend;
use imt_net::imap::PasswordProvider;
use imt_net::{ImapBackend, Pop3Backend};

/// Construct the receiving backend for `account`, boxed behind the
/// `MailBackend` trait object so callers are protocol-agnostic.
pub fn make_backend(account: &Account, provider: PasswordProvider) -> Box<dyn MailBackend> {
    match account.imap.protocol {
        IncomingProtocol::Imap => Box::new(ImapBackend::new(account.clone(), provider)),
        IncomingProtocol::Pop3 => Box::new(Pop3Backend::new(account.clone(), provider)),
    }
}
