//! SMTP sender built on `lettre` and an RFC 822 builder using `mail-builder`.

use std::str::FromStr;
use std::sync::Arc;

use lettre::address::Envelope;
use lettre::transport::smtp::authentication::{Credentials, Mechanism};
use lettre::transport::smtp::AsyncSmtpTransport;
use lettre::{AsyncTransport, Tokio1Executor};
use mail_builder::headers::address::Address as BuilderAddress;
use mail_builder::headers::message_id::MessageId as BuilderMessageId;
use mail_builder::MessageBuilder;

use imt_core::{Account, Address, AuthMethod, Tls};

use crate::error::{NetError, Result};

/// Provides the password for a given username on demand.
pub type PasswordProvider = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// A draft to be serialised to RFC 822 by `SmtpSender::build_rfc822`.
#[derive(Debug, Clone)]
pub struct BuildDraft {
    /// Sender (RFC 5322 From).
    pub from: Address,
    /// Primary recipients.
    pub to: Vec<Address>,
    /// Carbon-copy recipients.
    pub cc: Vec<Address>,
    /// Blind carbon-copy recipients.
    pub bcc: Vec<Address>,
    /// Subject line.
    pub subject: String,
    /// Plain-text body. Callers may pre-render HTML to text if needed.
    pub body_text: String,
    /// Attachments to include in the message.
    pub attachments: Vec<DraftAttachmentRef>,
    /// Optional `In-Reply-To` header.
    pub in_reply_to: Option<String>,
    /// Optional `References` header values.
    pub references: Vec<String>,
}

/// A single attachment owned by the caller and referenced by the builder.
#[derive(Debug, Clone)]
pub struct DraftAttachmentRef {
    /// Suggested filename presented to recipients.
    pub filename: String,
    /// MIME type (e.g. `application/pdf`).
    pub mime: String,
    /// Raw bytes.
    pub bytes: Vec<u8>,
}

/// SMTP submission client tied to a single account.
pub struct SmtpSender {
    account: Account,
    password_provider: PasswordProvider,
}

impl SmtpSender {
    /// Construct a new sender; the transport is built on each `send` call.
    pub fn new(account: Account, password_provider: PasswordProvider) -> Self {
        Self {
            account,
            password_provider,
        }
    }

    fn build_transport(&self) -> Result<AsyncSmtpTransport<Tokio1Executor>> {
        let host = self.account.smtp.host.as_str();
        let mut builder = match self.account.smtp.tls {
            Tls::Implicit => AsyncSmtpTransport::<Tokio1Executor>::relay(host)
                .map_err(|e| NetError::Tls(e.to_string()))?,
            Tls::StartTls => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(host)
                .map_err(|e| NetError::Tls(e.to_string()))?,
            Tls::None => AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(host),
        };
        builder = builder.port(self.account.smtp.port);

        match &self.account.smtp.auth {
            AuthMethod::Password { username } => {
                let password = (self.password_provider)(username).ok_or_else(|| {
                    NetError::Auth(format!("no password available for {}", username))
                })?;
                builder = builder.credentials(Credentials::new(username.clone(), password));
            }
            AuthMethod::OAuth2 { username, .. } => {
                let access_token = (self.password_provider)(username).ok_or_else(|| {
                    NetError::Auth(format!("no oauth access token available for {}", username))
                })?;
                builder = builder
                    .credentials(Credentials::new(username.clone(), access_token))
                    .authentication(vec![Mechanism::Xoauth2]);
            }
        }

        Ok(builder.build())
    }

    /// Send a raw RFC 822 message via SMTP.
    pub async fn send(
        &self,
        from: &Address,
        to: &[Address],
        cc: &[Address],
        bcc: &[Address],
        raw_rfc822: &[u8],
    ) -> Result<()> {
        if matches!(self.account.smtp.tls, imt_core::Tls::None) {
            tracing::warn!(
                target: "imt-net::smtp",
                "SMTP connection to {} is using PLAINTEXT (Tls::None) - credentials transmitted unencrypted",
                self.account.smtp.host
            );
        }
        let from_lettre = lettre::Address::from_str(&from.email)
            .map_err(|e| NetError::Parse(format!("invalid from address: {}", e)))?;

        let mut recipients = Vec::with_capacity(to.len() + cc.len() + bcc.len());
        for a in to.iter().chain(cc.iter()).chain(bcc.iter()) {
            let parsed = lettre::Address::from_str(&a.email)
                .map_err(|e| NetError::Parse(format!("invalid recipient {}: {}", a.email, e)))?;
            recipients.push(parsed);
        }

        let envelope = Envelope::new(Some(from_lettre), recipients)
            .map_err(|e| NetError::Parse(e.to_string()))?;

        let transport = self.build_transport()?;
        transport
            .send_raw(&envelope, raw_rfc822)
            .await
            .map_err(|e| NetError::Protocol(e.to_string()))?;
        Ok(())
    }
}

fn to_builder_address(a: &Address) -> BuilderAddress<'static> {
    let email = a.email.clone();
    match &a.name {
        Some(name) if !name.is_empty() => BuilderAddress::new_address(Some(name.clone()), email),
        _ => BuilderAddress::new_address(None::<String>, email),
    }
}

fn to_builder_address_list(list: &[Address]) -> BuilderAddress<'static> {
    let items: Vec<BuilderAddress<'static>> = list.iter().map(to_builder_address).collect();
    BuilderAddress::new_list(items)
}

/// Build an RFC 822 message from a `BuildDraft`.
pub fn build_rfc822(draft: &BuildDraft) -> Result<Vec<u8>> {
    let mut builder = MessageBuilder::new()
        .from(to_builder_address(&draft.from))
        .subject(draft.subject.clone())
        .text_body(draft.body_text.clone());

    if !draft.to.is_empty() {
        builder = builder.to(to_builder_address_list(&draft.to));
    }
    if !draft.cc.is_empty() {
        builder = builder.cc(to_builder_address_list(&draft.cc));
    }
    if !draft.bcc.is_empty() {
        builder = builder.bcc(to_builder_address_list(&draft.bcc));
    }
    if let Some(irt) = &draft.in_reply_to {
        builder = builder.in_reply_to(BuilderMessageId::new(irt.clone()));
    }
    if !draft.references.is_empty() {
        let refs: Vec<String> = draft.references.clone();
        builder = builder.references(BuilderMessageId::new_list(refs.into_iter()));
    }
    for att in &draft.attachments {
        builder = builder.attachment(att.mime.clone(), att.filename.clone(), att.bytes.clone());
    }

    builder
        .write_to_vec()
        .map_err(|e| NetError::other(format!("build rfc822: {}", e)))
}
