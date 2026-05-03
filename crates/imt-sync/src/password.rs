//! Password providers backed by the OS keyring (`imt_store::secrets`).

use std::sync::Arc;

use imt_core::AccountId;
use imt_store::secrets;

/// Build an IMAP password provider that reads `imap_password` from the keyring.
pub fn imap_provider_for(account_id: AccountId) -> imt_net::imap::PasswordProvider {
    Arc::new(move |_username: &str| secrets::load(account_id, "imap_password"))
}

/// Build an SMTP password provider that reads `smtp_password` from the keyring,
/// falling back to `imap_password` (most users reuse the same secret).
pub fn smtp_provider_for(account_id: AccountId) -> imt_net::smtp::PasswordProvider {
    Arc::new(move |_username: &str| {
        secrets::load(account_id, "smtp_password")
            .or_else(|| secrets::load(account_id, "imap_password"))
    })
}
