//! CLI subcommand handlers (account management).

use std::io::{self, Write};
use std::path::Path;

use anyhow::{Context, Result};
use imt_core::{AccountId, AuthMethod, NewAccountForm, Tls};
use imt_store::{secrets, AccountRepo, Db};
use imt_sync::SyncEngine;

fn prompt(label: &str) -> Result<String> {
    print!("{}: ", label);
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    Ok(s.trim().to_string())
}

fn prompt_default(label: &str, default: &str) -> Result<String> {
    print!("{} [{}]: ", label, default);
    io::stdout().flush()?;
    let mut s = String::new();
    io::stdin().read_line(&mut s)?;
    let v = s.trim();
    Ok(if v.is_empty() { default.to_string() } else { v.to_string() })
}

fn prompt_tls(label: &str) -> Result<Tls> {
    let v = prompt_default(label, "implicit")?;
    Ok(match v.to_lowercase().as_str() {
        "starttls" => Tls::StartTls,
        "none" => Tls::None,
        _ => Tls::Implicit,
    })
}

/// Optional CLI overrides for `add-account`. Any field set on the command
/// line skips its corresponding prompt.
#[derive(Default, Debug)]
pub struct AddAccountArgs {
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub username: Option<String>,
    pub imap_host: Option<String>,
    pub imap_port: u16,
    pub imap_tls: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_tls: String,
}

fn parse_tls(s: &str) -> Tls {
    match s.to_lowercase().as_str() {
        "starttls" => Tls::StartTls,
        "none" => Tls::None,
        _ => Tls::Implicit,
    }
}

/// `add-account` command (interactive when args are missing).
pub async fn add_account(db_path: &Path, args: AddAccountArgs) -> Result<()> {
    let db = Db::open(db_path).await.context("opening database")?;
    let email = match args.email {
        Some(v) => v,
        None => prompt("Email")?,
    };
    let display_name = match args.display_name {
        Some(v) => v,
        None => prompt_default("Display name", &email)?,
    };
    let username = match args.username {
        Some(v) => v,
        None => prompt_default("IMAP username", &email)?,
    };
    let password = match std::env::var("IMT_PASSWORD") {
        Ok(p) if !p.is_empty() => p,
        _ => rpassword::prompt_password("Password: ")?,
    };

    let (default_imap, default_imap_port, default_smtp, default_smtp_port) = preset_for_email(&email);
    let imap_host = match args.imap_host {
        Some(v) => v,
        None => prompt_default("IMAP host", &default_imap)?,
    };
    let imap_port = if args.imap_port != 0 { args.imap_port } else {
        prompt_default("IMAP port", &default_imap_port.to_string())?.parse().context("invalid IMAP port")?
    };
    let imap_tls = if !args.imap_tls.is_empty() { parse_tls(&args.imap_tls) } else { prompt_tls("IMAP TLS (implicit/starttls/none)")? };
    let smtp_host = match args.smtp_host {
        Some(v) => v,
        None => prompt_default("SMTP host", &default_smtp)?,
    };
    let smtp_port = if args.smtp_port != 0 { args.smtp_port } else {
        prompt_default("SMTP port", &default_smtp_port.to_string())?.parse().context("invalid SMTP port")?
    };
    let smtp_tls = if !args.smtp_tls.is_empty() { parse_tls(&args.smtp_tls) } else { prompt_tls("SMTP TLS (implicit/starttls/none)")? };

    let form = NewAccountForm {
        display_name,
        email,
        imap_host,
        imap_port,
        imap_tls,
        smtp_host,
        smtp_port,
        smtp_tls,
        username,
        password: password.clone(),
        oauth_client_id: String::new(),
        oauth_client_secret: String::new(),
        oauth_code: String::new(),
        oauth_verifier: String::new(),
        oauth_redirect_uri: String::new(),
    };

    let order = AccountRepo::new(db.pool()).list().await?.len() as i32;
    let account = form.into_account(order);
    let id = account.id;

    AccountRepo::new(db.pool()).upsert(&account).await?;
    secrets::store(id, "imap_password", &password);
    secrets::store(id, "smtp_password", &password);

    println!("Added account {} ({})", account.display_name, account.address.email);
    println!("  id: {}", id.0);
    Ok(())
}

/// `list-accounts` command.
pub async fn list_accounts(db_path: &Path) -> Result<()> {
    let db = Db::open(db_path).await?;
    let accounts = AccountRepo::new(db.pool()).list().await?;
    if accounts.is_empty() {
        println!("No accounts configured. Run `imt add-account` to add one.");
        return Ok(());
    }
    for a in &accounts {
        let user = match &a.imap.auth {
            AuthMethod::Password { username } => username.clone(),
            AuthMethod::OAuth2 { username, .. } => format!("{} (oauth2)", username),
        };
        println!(
            "{:<24} {:<32} imap={}:{} smtp={}:{} user={}",
            a.display_name, a.address.email, a.imap.host, a.imap.port, a.smtp.host, a.smtp.port, user
        );
    }
    Ok(())
}

/// `delete-account` command.
pub async fn delete_account(db_path: &Path, id_str: &str) -> Result<()> {
    let id = uuid::Uuid::parse_str(id_str).context("invalid account id (expected UUID)")?;
    let id = AccountId(id);
    let db = std::sync::Arc::new(Db::open(db_path).await?);
    let (engine, _rx) = SyncEngine::new(db.clone());
    engine.remove_account(id).await?;
    println!("Deleted account {}", id.0);
    Ok(())
}

/// Standard provider presets keyed off the email domain.
pub fn preset_for_email(email: &str) -> (String, u16, String, u16) {
    let domain = email.split('@').nth(1).unwrap_or("").to_lowercase();
    match domain.as_str() {
        "gmail.com" | "googlemail.com" => ("imap.gmail.com".into(), 993, "smtp.gmail.com".into(), 465),
        "outlook.com" | "hotmail.com" | "live.com" | "office365.com" => {
            ("outlook.office365.com".into(), 993, "smtp.office365.com".into(), 587)
        }
        "fastmail.com" | "fastmail.fm" => ("imap.fastmail.com".into(), 993, "smtp.fastmail.com".into(), 465),
        "yahoo.com" => ("imap.mail.yahoo.com".into(), 993, "smtp.mail.yahoo.com".into(), 465),
        "icloud.com" | "me.com" | "mac.com" => ("imap.mail.me.com".into(), 993, "smtp.mail.me.com".into(), 587),
        _ => ("".into(), 993, "".into(), 465),
    }
}
