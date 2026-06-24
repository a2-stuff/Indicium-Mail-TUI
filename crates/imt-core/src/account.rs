use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::Address;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

impl AccountId {
    pub fn new() -> Self { Self(Uuid::new_v4()) }
}

impl Default for AccountId {
    fn default() -> Self { Self::new() }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tls {
    /// Implicit TLS from the start (port 993 / 465).
    Implicit,
    /// Plain connection upgraded via STARTTLS (port 143 / 587).
    StartTls,
    /// No encryption. Local/dev only.
    None,
}

/// Which protocol is used to receive mail. SMTP is always used for sending.
///
/// `Imap` is the default and leaves messages on the server with full folder,
/// flag, and IDLE support. `Pop3` is a download-oriented protocol with a single
/// INBOX, no server-side flags, and no IDLE (the worker polls instead). POP3
/// pairs naturally with [`Account::keep_on_server`] = false to download and
/// remove mail from the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum IncomingProtocol {
    /// IMAP (default): folders, flags, IDLE, leaves mail on the server.
    #[default]
    Imap,
    /// POP3: single INBOX, download-oriented, no server flags/IDLE.
    Pop3,
}

impl IncomingProtocol {
    /// Short upper-case label for menus and the onboarding cycler.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Imap => "IMAP",
            Self::Pop3 => "POP3",
        }
    }

    /// The conventional implicit-TLS port for this protocol (993 / 995).
    pub fn default_tls_port(&self) -> u16 {
        match self {
            Self::Imap => 993,
            Self::Pop3 => 995,
        }
    }
}

/// OAuth2 provider type stored in account config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OAuthProvider {
    Google,
    Microsoft { tenant: String },
    Yahoo,
    /// Fully custom provider with explicit endpoints and scope.
    Custom {
        auth_url: String,
        token_url: String,
        scope: String,
    },
}

impl OAuthProvider {
    /// Best-guess provider from the IMAP hostname.
    pub fn from_imap_host(host: &str) -> Option<Self> {
        match host {
            h if h.contains("gmail.com") || h.contains("googlemail.com") => Some(Self::Google),
            h if h.contains("outlook") || h.contains("office365") || h.contains("microsoft") => {
                Some(Self::Microsoft { tenant: "common".to_string() })
            }
            h if h.contains("yahoo") => Some(Self::Yahoo),
            _ => None,
        }
    }

    pub fn label(&self) -> &str {
        match self {
            Self::Google => "Google / Gmail",
            Self::Microsoft { .. } => "Microsoft / Office 365",
            Self::Yahoo => "Yahoo Mail",
            Self::Custom { .. } => "Custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    Password { username: String },
    /// OAuth2 XOAUTH2. Tokens live in the secrets store; only metadata here.
    OAuth2 {
        username: String,
        provider: OAuthProvider,
        /// OAuth2 client_id registered by the user.
        client_id: String,
    },
}

impl AuthMethod {
    pub fn username(&self) -> &str {
        match self {
            Self::Password { username } | Self::OAuth2 { username, .. } => username,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub tls: Tls,
    pub auth: AuthMethod,
    /// Receiving protocol for this account (IMAP or POP3). Stored here so it
    /// round-trips through the serialized `imap_json` column with no migration;
    /// old rows without the field deserialize to `Imap`.
    #[serde(default)]
    pub protocol: IncomingProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub tls: Tls,
    pub auth: AuthMethod,
}

/// Default for `keep_on_server` (true = leave a copy on the server).
fn default_keep_on_server() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub display_name: String,
    pub address: Address,
    pub imap: ImapConfig,
    pub smtp: SmtpConfig,
    /// Display order in the sidebar.
    pub order: i32,
    /// When false, a message is deleted from the IMAP server once its full body
    /// has been downloaded locally (POP3-style "do not leave a copy"). Defaults
    /// to true so nothing is removed from the server unless the user opts in.
    #[serde(default = "default_keep_on_server")]
    pub keep_on_server: bool,
}

/// User-supplied form data when adding a new account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAccountForm {
    pub display_name: String,
    pub email: String,
    /// Receiving protocol (IMAP default, or POP3).
    #[serde(default)]
    pub incoming_protocol: IncomingProtocol,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: Tls,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: Tls,
    pub username: String,
    /// Plaintext password. Only used when auth_type is Password.
    pub password: String,
    /// OAuth2 client_id. Non-empty selects OAuth2 auth.
    pub oauth_client_id: String,
    /// OAuth2 client_secret (optional; leave empty for PKCE-only flows).
    pub oauth_client_secret: String,
    /// OAuth2 authorization code to exchange for tokens.
    pub oauth_code: String,
    /// PKCE code verifier that was used when generating the auth URL.
    pub oauth_verifier: String,
    /// Redirect URI used when generating the auth URL.
    pub oauth_redirect_uri: String,
    /// Leave a copy of downloaded messages on the server (default true).
    #[serde(default = "default_keep_on_server")]
    pub keep_on_server: bool,
}

impl NewAccountForm {
    pub fn defaults() -> Self {
        Self {
            display_name: String::new(),
            email: String::new(),
            incoming_protocol: IncomingProtocol::Imap,
            imap_host: String::new(),
            imap_port: 993,
            imap_tls: Tls::Implicit,
            smtp_host: String::new(),
            smtp_port: 465,
            smtp_tls: Tls::Implicit,
            username: String::new(),
            password: String::new(),
            oauth_client_id: String::new(),
            oauth_client_secret: String::new(),
            oauth_code: String::new(),
            oauth_verifier: String::new(),
            oauth_redirect_uri: String::new(),
            keep_on_server: true,
        }
    }

    /// Whether this form is configured for OAuth2 (vs password) auth.
    pub fn is_oauth2(&self) -> bool {
        !self.oauth_client_id.is_empty()
    }

    pub fn into_account(self, order: i32) -> Account {
        let auth = if self.is_oauth2() {
            let provider = OAuthProvider::from_imap_host(&self.imap_host)
                .unwrap_or(OAuthProvider::Custom {
                    auth_url: String::new(),
                    token_url: String::new(),
                    scope: String::new(),
                });
            AuthMethod::OAuth2 {
                username: self.username.clone(),
                provider,
                client_id: self.oauth_client_id.clone(),
            }
        } else {
            AuthMethod::Password { username: self.username.clone() }
        };
        let display = if self.display_name.is_empty() { self.email.clone() } else { self.display_name.clone() };
        Account {
            id: AccountId::new(),
            display_name: display.clone(),
            address: Address::named(display, self.email),
            imap: ImapConfig { host: self.imap_host, port: self.imap_port, tls: self.imap_tls, auth: auth.clone(), protocol: self.incoming_protocol },
            smtp: SmtpConfig { host: self.smtp_host, port: self.smtp_port, tls: self.smtp_tls, auth },
            order,
            keep_on_server: self.keep_on_server,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imap_config_without_protocol_field_defaults_to_imap() {
        // Old persisted `imap_json` rows predate the protocol field; they must
        // deserialize as IMAP so existing accounts keep working.
        let legacy = r#"{"host":"imap.example.com","port":993,"tls":"Implicit","auth":{"Password":{"username":"me@example.com"}}}"#;
        let cfg: ImapConfig = serde_json::from_str(legacy).unwrap();
        assert_eq!(cfg.protocol, IncomingProtocol::Imap);
    }

    #[test]
    fn imap_config_protocol_roundtrips_pop3() {
        let cfg = ImapConfig {
            host: "pop.example.com".into(),
            port: 995,
            tls: Tls::Implicit,
            auth: AuthMethod::Password { username: "me@example.com".into() },
            protocol: IncomingProtocol::Pop3,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let back: ImapConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.protocol, IncomingProtocol::Pop3);
    }

    #[test]
    fn into_account_propagates_protocol_and_defaults() {
        // The form default is IMAP.
        assert_eq!(NewAccountForm::defaults().incoming_protocol, IncomingProtocol::Imap);

        let mut form = NewAccountForm::defaults();
        form.email = "me@example.com".into();
        form.imap_host = "pop.example.com".into();
        form.smtp_host = "smtp.example.com".into();
        form.incoming_protocol = IncomingProtocol::Pop3;
        let acc = form.into_account(0);
        assert_eq!(acc.imap.protocol, IncomingProtocol::Pop3);
    }

    #[test]
    fn protocol_default_ports() {
        assert_eq!(IncomingProtocol::Imap.default_tls_port(), 993);
        assert_eq!(IncomingProtocol::Pop3.default_tls_port(), 995);
    }
}
