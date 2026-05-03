//! Common provider presets for account onboarding.

use imt_core::{NewAccountForm, Tls};

/// Return a partially-filled `NewAccountForm` for known providers based on the
/// email's domain, or `None` if the domain is unknown.
///
/// The returned form has the IMAP/SMTP host, port and TLS fields populated.
/// All other string fields are empty so callers can selectively merge.
pub fn preset_for(email: &str) -> Option<NewAccountForm> {
    let domain = email.split('@').nth(1)?.to_ascii_lowercase();
    let (imap_host, imap_port, imap_tls, smtp_host, smtp_port, smtp_tls) = match domain.as_str() {
        "gmail.com" | "googlemail.com" => (
            "imap.gmail.com",
            993,
            Tls::Implicit,
            "smtp.gmail.com",
            465,
            Tls::Implicit,
        ),
        "outlook.com" | "hotmail.com" | "live.com" | "msn.com" => (
            "outlook.office365.com",
            993,
            Tls::Implicit,
            "smtp.office365.com",
            587,
            Tls::StartTls,
        ),
        "fastmail.com" | "fastmail.fm" => (
            "imap.fastmail.com",
            993,
            Tls::Implicit,
            "smtp.fastmail.com",
            465,
            Tls::Implicit,
        ),
        "yahoo.com" | "yahoo.co.uk" | "ymail.com" => (
            "imap.mail.yahoo.com",
            993,
            Tls::Implicit,
            "smtp.mail.yahoo.com",
            465,
            Tls::Implicit,
        ),
        "icloud.com" | "me.com" | "mac.com" => (
            "imap.mail.me.com",
            993,
            Tls::Implicit,
            "smtp.mail.me.com",
            587,
            Tls::StartTls,
        ),
        _ => return None,
    };
    Some(NewAccountForm {
        display_name: String::new(),
        email: String::new(),
        imap_host: imap_host.to_string(),
        imap_port,
        imap_tls,
        smtp_host: smtp_host.to_string(),
        smtp_port,
        smtp_tls,
        username: String::new(),
        password: String::new(),
    })
}
