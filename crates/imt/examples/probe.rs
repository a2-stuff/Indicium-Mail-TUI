// One-shot sync probe. Connects to the configured account, syncs INBOX once,
// prints what was fetched. Used to diagnose why envelopes aren't persisted.

use std::sync::Arc;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::new("info,imt_sync=debug,imt_net=debug"))
        .init();

    let dirs = directories::ProjectDirs::from("dev", "indicium", "indicium-mail-tui").unwrap();
    let db_path = dirs.data_local_dir().join("imt.sqlite3");
    let db = Arc::new(imt_store::Db::open(&db_path).await?);

    let accounts = imt_store::AccountRepo::new(db.pool()).list().await?;
    let acc = accounts.into_iter().next().expect("no account");
    println!("account: {} <{}>", acc.display_name, acc.address.email);

    let folders = imt_store::FolderRepo::new(db.pool()).list_by_account(acc.id).await?;
    let inbox = folders.iter().find(|f| f.role == imt_core::FolderRole::Inbox).expect("no INBOX");
    println!("INBOX folder: id={} uid_next={} message_count={}", inbox.id.0, inbox.uid_next, inbox.message_count);

    let pwd = imt_store::secrets::load(acc.id, "imap_password").expect("no password");
    println!("password loaded ({} chars)", pwd.len());

    let provider: imt_net::imap::PasswordProvider = Arc::new(move |_| Some(pwd.clone()));
    let mut backend = imt_net::ImapBackend::new(acc.clone(), provider);
    use imt_net::backend::MailBackend;
    backend.connect().await?;
    println!("connected");

    let info = backend.list_folders().await?;
    for f in &info {
        println!("  folder: {} role={:?} count={} uid_next={}", f.path, f.role, f.message_count, f.uid_next);
    }

    let state = backend.select_folder("INBOX").await?;
    println!("INBOX state: exists={} unseen={} uid_validity={} uid_next={}", state.exists, state.unseen, state.uid_validity, state.uid_next);

    let envelopes = backend.fetch_envelopes("INBOX", imt_net::backend::UidRange::All).await?;
    println!("fetched {} envelopes via UidRange::All", envelopes.len());
    for e in &envelopes {
        println!("  uid={} subject={:?} date={}", e.uid, e.headers.subject, e.internal_date);
    }

    let envelopes2 = backend.fetch_envelopes("INBOX", imt_net::backend::UidRange::Range(1, 13)).await?;
    println!("fetched {} envelopes via Range(1,13)", envelopes2.len());

    backend.disconnect().await?;
    Ok(())
}
