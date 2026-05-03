//! OAuth2 Authorization Code + Refresh flow for IMAP/SMTP XOAUTH2.
//!
//! Provides a small, dependency-light implementation that supports Gmail and
//! Microsoft 365. Token exchange is performed via `reqwest` and PKCE is
//! generated locally with `rand` + `sha2`.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use url::Url;

use crate::error::{NetError, Result};

/// Identifies which OAuth2 provider a flow is targeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OAuthProvider {
    /// Google (Gmail) OAuth2 endpoints.
    Google,
    /// Microsoft Identity Platform with the given tenant (use `common` if unknown).
    Microsoft {
        /// Azure AD tenant; `common`, `consumers`, `organizations`, or a tenant id.
        tenant: String,
    },
}

impl OAuthProvider {
    /// URL of the authorization endpoint.
    pub fn auth_url(&self) -> &'static str {
        match self {
            OAuthProvider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            OAuthProvider::Microsoft { .. } => "",
        }
    }

    /// Owned authorization endpoint (Microsoft varies by tenant).
    pub fn auth_url_string(&self) -> String {
        match self {
            OAuthProvider::Google => self.auth_url().to_string(),
            OAuthProvider::Microsoft { tenant } => format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
                tenant
            ),
        }
    }

    /// Owned token endpoint.
    pub fn token_url_string(&self) -> String {
        match self {
            OAuthProvider::Google => "https://oauth2.googleapis.com/token".to_string(),
            OAuthProvider::Microsoft { tenant } => format!(
                "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
                tenant
            ),
        }
    }

    /// Default scopes required for IMAP+SMTP XOAUTH2 access.
    pub fn scopes(&self) -> Vec<String> {
        match self {
            OAuthProvider::Google => vec!["https://mail.google.com/".to_string()],
            OAuthProvider::Microsoft { .. } => vec![
                "offline_access".to_string(),
                "https://outlook.office.com/IMAP.AccessAsUser.All".to_string(),
                "https://outlook.office.com/SMTP.Send".to_string(),
            ],
        }
    }
}

/// PKCE code verifier (the secret kept by the client).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PkceVerifier(pub String);

impl PkceVerifier {
    /// Generate a fresh random verifier.
    pub fn new_random() -> Self {
        let mut bytes = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// SHA256 challenge derived from this verifier.
    pub fn challenge(&self) -> String {
        let digest = Sha256::digest(self.0.as_bytes());
        URL_SAFE_NO_PAD.encode(digest)
    }
}

/// Opaque CSRF state token round-tripped through the authorization endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsrfToken(pub String);

impl CsrfToken {
    /// Generate a fresh random CSRF token.
    pub fn new_random() -> Self {
        let mut bytes = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }
}

/// Tokens returned by an authorization-code or refresh-token exchange.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokens {
    /// Short-lived bearer access token.
    pub access_token: String,
    /// Long-lived refresh token (may be `None` on refresh exchanges that do not rotate it).
    pub refresh_token: Option<String>,
    /// Absolute UTC time after which the access token must be refreshed.
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
}

/// Stateful handle that knows how to drive the OAuth2 Authorization Code flow.
pub struct OAuthFlow {
    provider: OAuthProvider,
    client_id: String,
    client_secret: Option<String>,
    http: reqwest::Client,
}

impl OAuthFlow {
    /// Construct a new flow for `provider` using the given client credentials.
    pub fn new(provider: OAuthProvider, client_id: String, client_secret: Option<String>) -> Self {
        Self {
            provider,
            client_id,
            client_secret,
            http: reqwest::Client::new(),
        }
    }

    /// Build the URL the user should open, plus the verifier and CSRF token to
    /// remember until the redirect arrives.
    pub fn authorize_url(
        &self,
        redirect_uri: &str,
        login_hint: Option<&str>,
    ) -> (Url, PkceVerifier, CsrfToken) {
        let verifier = PkceVerifier::new_random();
        let challenge = verifier.challenge();
        let state = CsrfToken::new_random();
        let scope = self.provider.scopes().join(" ");

        let mut url = Url::parse(&self.provider.auth_url_string())
            .unwrap_or_else(|_| Url::parse("https://invalid.invalid/").expect("static url"));
        {
            let mut qp = url.query_pairs_mut();
            qp.append_pair("response_type", "code")
                .append_pair("client_id", &self.client_id)
                .append_pair("redirect_uri", redirect_uri)
                .append_pair("scope", &scope)
                .append_pair("state", &state.0)
                .append_pair("code_challenge", &challenge)
                .append_pair("code_challenge_method", "S256")
                .append_pair("access_type", "offline")
                .append_pair("prompt", "consent");
            if let Some(hint) = login_hint {
                qp.append_pair("login_hint", hint);
            }
        }
        (url, verifier, state)
    }

    /// Exchange an authorization `code` for access and refresh tokens.
    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: PkceVerifier,
        redirect_uri: &str,
    ) -> Result<OAuthTokens> {
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "authorization_code".to_string()),
            ("code", code.to_string()),
            ("redirect_uri", redirect_uri.to_string()),
            ("client_id", self.client_id.clone()),
            ("code_verifier", verifier.0),
        ];
        if let Some(secret) = &self.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        self.post_token(&form).await
    }

    /// Exchange a `refresh_token` for a new access token (and possibly a rotated refresh token).
    pub async fn refresh(&self, refresh_token: &str) -> Result<OAuthTokens> {
        let mut form: Vec<(&str, String)> = vec![
            ("grant_type", "refresh_token".to_string()),
            ("refresh_token", refresh_token.to_string()),
            ("client_id", self.client_id.clone()),
        ];
        if let Some(secret) = &self.client_secret {
            form.push(("client_secret", secret.clone()));
        }
        let mut tokens = self.post_token(&form).await?;
        if tokens.refresh_token.is_none() {
            tokens.refresh_token = Some(refresh_token.to_string());
        }
        Ok(tokens)
    }

    async fn post_token(&self, form: &[(&str, String)]) -> Result<OAuthTokens> {
        let url = self.provider.token_url_string();
        let resp = self
            .http
            .post(&url)
            .form(form)
            .send()
            .await
            .map_err(|e| NetError::other(format!("oauth token request: {}", e)))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| NetError::other(format!("oauth token body: {}", e)))?;
        if !status.is_success() {
            return Err(NetError::Auth(format!(
                "oauth token endpoint {}: {}",
                status, body
            )));
        }
        let parsed: TokenResponse = serde_json::from_str(&body)
            .map_err(|e| NetError::Parse(format!("oauth token json: {}: {}", e, body)))?;
        let expires_in = parsed.expires_in.unwrap_or(3600);
        let expires_at = Utc::now() + ChronoDuration::seconds(expires_in.max(0));
        Ok(OAuthTokens {
            access_token: parsed.access_token,
            refresh_token: parsed.refresh_token,
            expires_at,
        })
    }
}

/// Build the SASL initial-response string for IMAP/SMTP XOAUTH2.
///
/// The returned value is the base64-encoded byte string
/// `user=USER\x01auth=Bearer TOKEN\x01\x01`.
pub fn xoauth2_sasl(username: &str, access_token: &str) -> String {
    let raw = format!("user={}\x01auth=Bearer {}\x01\x01", username, access_token);
    STANDARD.encode(raw.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sasl_format() {
        let s = xoauth2_sasl("a@b", "tok");
        let raw = STANDARD.decode(s).expect("base64");
        assert_eq!(raw, b"user=a@b\x01auth=Bearer tok\x01\x01");
    }

    #[test]
    fn pkce_challenge_is_url_safe() {
        let v = PkceVerifier::new_random();
        let c = v.challenge();
        assert!(!c.contains('+') && !c.contains('/') && !c.contains('='));
    }
}

