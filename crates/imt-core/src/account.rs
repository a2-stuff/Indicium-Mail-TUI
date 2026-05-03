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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AuthMethod {
    Password { username: String },
    OAuth2 { username: String, refresh_token_ref: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImapConfig {
    pub host: String,
    pub port: u16,
    pub tls: Tls,
    pub auth: AuthMethod,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub tls: Tls,
    pub auth: AuthMethod,
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
}

/// User-supplied form data when adding a new account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAccountForm {
    pub display_name: String,
    pub email: String,
    pub imap_host: String,
    pub imap_port: u16,
    pub imap_tls: Tls,
    pub smtp_host: String,
    pub smtp_port: u16,
    pub smtp_tls: Tls,
    pub username: String,
    /// Plaintext password. Caller is responsible for forwarding to a secret store.
    pub password: String,
}

impl NewAccountForm {
    /// Reasonable defaults for a brand-new form (implicit TLS on standard ports).
    pub fn defaults() -> Self {
        Self {
            display_name: String::new(),
            email: String::new(),
            imap_host: String::new(),
            imap_port: 993,
            imap_tls: Tls::Implicit,
            smtp_host: String::new(),
            smtp_port: 465,
            smtp_tls: Tls::Implicit,
            username: String::new(),
            password: String::new(),
        }
    }

    /// Build a domain `Account` from the form (excluding the password, which is stored
    /// separately by the caller).
    pub fn into_account(self, order: i32) -> Account {
        let auth = AuthMethod::Password { username: self.username.clone() };
        let display = if self.display_name.is_empty() { self.email.clone() } else { self.display_name.clone() };
        Account {
            id: AccountId::new(),
            display_name: display.clone(),
            address: Address::named(display, self.email),
            imap: ImapConfig { host: self.imap_host, port: self.imap_port, tls: self.imap_tls, auth: auth.clone() },
            smtp: SmtpConfig { host: self.smtp_host, port: self.smtp_port, tls: self.smtp_tls, auth },
            order,
        }
    }
}
