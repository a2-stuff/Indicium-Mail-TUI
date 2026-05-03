//! Credential providers backed by the OS keyring (`imt_store::secrets`).
//! For OAuth2 accounts the provider returns the cached access token, which
//! must be freshened by `ensure_fresh_tokens` before each connect attempt.

use std::sync::Arc;

use imt_core::{Account, AccountId, AuthMethod};
use imt_store::secrets;

/// Build an IMAP password provider for the account.
/// OAuth2 accounts return the cached access token from secrets.
pub fn imap_provider_for(account: &Account) -> imt_net::imap::PasswordProvider {
    let account_id = account.id;
    match &account.imap.auth {
        AuthMethod::Password { .. } => {
            Arc::new(move |_| secrets::load(account_id, "imap_password"))
        }
        AuthMethod::OAuth2 { .. } => {
            Arc::new(move |_| secrets::load(account_id, "oauth_access_token"))
        }
    }
}

/// Build an SMTP password provider for the account.
pub fn smtp_provider_for(account: &Account) -> imt_net::smtp::PasswordProvider {
    let account_id = account.id;
    match &account.smtp.auth {
        AuthMethod::Password { .. } => Arc::new(move |_| {
            secrets::load(account_id, "smtp_password")
                .or_else(|| secrets::load(account_id, "imap_password"))
        }),
        AuthMethod::OAuth2 { .. } => {
            Arc::new(move |_| secrets::load(account_id, "oauth_access_token"))
        }
    }
}

/// If the account uses OAuth2 and the stored access token is expiring within
/// 60 seconds, refresh it using the stored refresh token. No-op for password
/// accounts.
pub async fn ensure_fresh_tokens(account: &Account) -> anyhow::Result<()> {
    let (provider_core, client_id) = match &account.imap.auth {
        AuthMethod::Password { .. } => return Ok(()),
        AuthMethod::OAuth2 { provider, client_id, .. } => (provider.clone(), client_id.clone()),
    };

    // Check cached expiry; only refresh if needed.
    if let Some(expiry_str) = secrets::load(account.id, "oauth_access_expiry") {
        if let Ok(expiry) = expiry_str.parse::<i64>() {
            if chrono::Utc::now().timestamp() + 60 < expiry {
                return Ok(());
            }
        }
    }

    let refresh_token = secrets::load(account.id, "oauth_refresh_token").ok_or_else(|| {
        anyhow::anyhow!(
            "OAuth2 account has no refresh token - re-authorize via Accounts manager"
        )
    })?;

    let client_secret = secrets::load(account.id, "oauth_client_secret");
    let net_provider = imt_net::OAuthProvider::from_core(&provider_core);
    let flow = imt_net::OAuthFlow::new(net_provider, client_id, client_secret);
    let tokens = flow.refresh(&refresh_token).await?;

    secrets::store(account.id, "oauth_access_token", &tokens.access_token);
    secrets::store(
        account.id,
        "oauth_access_expiry",
        &tokens.expires_at.timestamp().to_string(),
    );
    if let Some(rotated) = &tokens.refresh_token {
        secrets::store(account.id, "oauth_refresh_token", rotated);
    }
    tracing::debug!("OAuth2 token refreshed for {}", account.id.0);
    Ok(())
}

/// Store initial password credentials for a new account.
pub fn store_password(account_id: AccountId, password: &str) {
    secrets::store(account_id, "imap_password", password);
    secrets::store(account_id, "smtp_password", password);
}

/// Delete all credentials (password and OAuth2) for an account.
pub fn delete_all(account_id: AccountId) {
    for key in &[
        "imap_password",
        "smtp_password",
        "oauth_access_token",
        "oauth_refresh_token",
        "oauth_access_expiry",
        "oauth_client_secret",
    ] {
        secrets::delete(account_id, key);
    }
}
