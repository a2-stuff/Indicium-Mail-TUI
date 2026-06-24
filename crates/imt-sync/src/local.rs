//! Local-only folder support for POP3 accounts.
//!
//! POP3 (RFC 1939) exposes a single maildrop and has no concept of folders, so
//! a POP3 account cannot have server-side Sent / Trash / Drafts. To behave like
//! a normal mail client we materialise those as local-only folders in the store:
//! sent mail is filed into the local Sent folder, deletes move the local row
//! into the local Trash folder, and the sync engine never tries to fetch them
//! from the server (they are not returned by `Pop3Backend::list_folders`).

use imt_core::{Account, Folder, FolderId, FolderRole, IncomingProtocol};
use imt_store::{Db, FolderRepo};

/// Local folders a POP3 account gets in addition to the server `INBOX`.
const POP3_LOCAL_FOLDERS: &[(&str, FolderRole)] = &[
    ("Sent", FolderRole::Sent),
    ("Drafts", FolderRole::Drafts),
    ("Trash", FolderRole::Trash),
];

/// Ensure a POP3 account has its local Sent / Drafts / Trash folders in the
/// store. No-op for IMAP accounts (their folders come from the server) and for
/// folders that already exist.
pub async fn ensure_pop3_local_folders(db: &Db, account: &Account) -> imt_store::Result<()> {
    if account.imap.protocol != IncomingProtocol::Pop3 {
        return Ok(());
    }
    let repo = FolderRepo::new(db.pool());
    let existing = repo.list_by_account(account.id).await?;
    for (path, role) in POP3_LOCAL_FOLDERS {
        if existing.iter().any(|f| f.path.eq_ignore_ascii_case(path)) {
            continue;
        }
        let folder = Folder {
            id: FolderId::new(),
            account_id: account.id,
            path: (*path).to_string(),
            name: (*path).to_string(),
            role: *role,
            uid_validity: 1,
            uid_next: 1,
            message_count: 0,
            unread_count: 0,
        };
        repo.upsert(&folder).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use imt_core::{AccountId, Address, AuthMethod, ImapConfig, SmtpConfig, Tls};
    use imt_store::AccountRepo;

    fn account(protocol: IncomingProtocol) -> Account {
        let auth = AuthMethod::Password { username: "me@example.com".into() };
        Account {
            id: AccountId::new(),
            display_name: "Me".into(),
            address: Address { name: Some("Me".into()), email: "me@example.com".into() },
            imap: ImapConfig { host: "h".into(), port: 995, tls: Tls::Implicit, auth: auth.clone(), protocol },
            smtp: SmtpConfig { host: "s".into(), port: 465, tls: Tls::Implicit, auth },
            order: 0,
            keep_on_server: true,
        }
    }

    #[tokio::test]
    async fn pop3_account_gets_local_folders_imap_does_not() {
        let path = std::env::temp_dir().join(format!("imt-local-{}.sqlite3", uuid::Uuid::new_v4().simple()));
        let db = Db::open(&path).await.unwrap();

        let pop = account(IncomingProtocol::Pop3);
        let imap = account(IncomingProtocol::Imap);
        AccountRepo::new(db.pool()).upsert(&pop).await.unwrap();
        AccountRepo::new(db.pool()).upsert(&imap).await.unwrap();

        ensure_pop3_local_folders(&db, &pop).await.unwrap();
        ensure_pop3_local_folders(&db, &imap).await.unwrap();

        let repo = FolderRepo::new(db.pool());
        let pop_folders = repo.list_by_account(pop.id).await.unwrap();
        assert!(pop_folders.iter().any(|f| f.role == FolderRole::Sent));
        assert!(pop_folders.iter().any(|f| f.role == FolderRole::Trash));
        assert!(pop_folders.iter().any(|f| f.role == FolderRole::Drafts));
        assert_eq!(pop_folders.len(), 3, "exactly Sent/Drafts/Trash");

        // IMAP accounts get nothing (folders come from the server).
        assert!(repo.list_by_account(imap.id).await.unwrap().is_empty());

        // Idempotent: a second call does not duplicate.
        ensure_pop3_local_folders(&db, &pop).await.unwrap();
        assert_eq!(repo.list_by_account(pop.id).await.unwrap().len(), 3);

        let _ = std::fs::remove_file(&path);
    }
}
