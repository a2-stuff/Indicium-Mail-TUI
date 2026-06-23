//! Data source abstraction and an in-memory mock implementation.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::{Duration, Utc};
use imt_core::{
    Account, AccountId, Address, AuthMethod, Draft, DraftId, Flag, Folder, FolderId, FolderRole,
    ImapConfig, Message, MessageBody, MessageHeaders, MessageId, NewAccountForm, SmtpConfig, Tls,
    Uid,
};

/// Abstract source of mail data the TUI talks to.
pub trait DataSource: Send + Sync {
    /// All configured accounts.
    fn accounts(&self) -> Vec<Account>;
    /// Folders for an account.
    fn folders(&self, account: AccountId) -> Vec<Folder>;
    /// Messages in a folder (envelopes).
    fn messages(&self, folder: FolderId) -> Vec<Message>;
    /// Full body for a message, fetched on demand.
    fn message_body(&self, message: MessageId) -> Option<MessageBody>;
    /// All messages in the same conversation as `message`, oldest-first.
    /// Default returns just the message itself (no threading).
    fn thread(&self, message: MessageId) -> Vec<Message> {
        let _ = message;
        Vec::new()
    }
    /// Persist a draft.
    fn save_draft(&self, draft: &Draft) -> anyhow::Result<()>;
    /// Send a draft via the corresponding account.
    fn send(&self, draft: &Draft) -> anyhow::Result<()>;
    /// Mark a message read.
    fn mark_read(&self, message: MessageId);
    /// Toggle the flagged state of a message.
    fn toggle_flag(&self, message: MessageId);
    /// Search for messages matching `query`. Returns matching message ids.
    fn search(&self, query: &str) -> Vec<MessageId>;
    /// Add a new account from an onboarding form. Default: not supported.
    fn add_account(&self, _form: NewAccountForm) -> anyhow::Result<AccountId> {
        anyhow::bail!("add_account not supported by this data source")
    }
    /// Update an existing account from a form. `password` is optional - if
    /// `None`, the existing stored password is kept. Default: not supported.
    fn update_account(&self, _id: AccountId, _form: NewAccountForm, _password_changed: bool) -> anyhow::Result<()> {
        anyhow::bail!("update_account not supported by this data source")
    }
    /// Delete an account and all its data. Default: not supported.
    fn delete_account(&self, _id: AccountId) -> anyhow::Result<()> {
        anyhow::bail!("delete_account not supported by this data source")
    }
    /// Set or unset the `\Seen` flag on a message.
    fn set_seen(&self, _message: MessageId, _seen: bool) {}
    /// Move a message to another folder. Default: no-op.
    fn move_message(&self, _message: MessageId, _dest_folder: FolderId) -> anyhow::Result<()> {
        Ok(())
    }
    /// Delete a message (typically by moving to Trash). Default: no-op.
    fn delete_message(&self, _message: MessageId) -> anyhow::Result<()> {
        Ok(())
    }
    /// Permanently delete every message in `folder` (intended for Trash).
    /// Default: no-op.
    fn empty_trash(&self, _folder: FolderId) -> anyhow::Result<()> {
        Ok(())
    }
    /// Trigger a sync. `None` arguments mean "all". Default: no-op.
    fn refresh(&self, _account: Option<AccountId>, _folder: Option<FolderId>) {}
    /// Current backend status string (e.g. "syncing", "idle", "connecting").
    /// Default: empty.
    fn status(&self) -> String { String::new() }
    /// Pop the next pending toast notification, if any. Default: None.
    fn pop_notification(&self) -> Option<String> { None }
}

#[derive(Default)]
struct InnerStore {
    accounts: Vec<Account>,
    folders: HashMap<AccountId, Vec<Folder>>,
    messages: HashMap<FolderId, Vec<Message>>,
    bodies: HashMap<MessageId, MessageBody>,
    drafts: Vec<Draft>,
}

/// In-memory `DataSource` with seeded sample data, useful for development.
#[derive(Clone)]
pub struct InMemoryDataSource {
    inner: Arc<Mutex<InnerStore>>,
}

impl InMemoryDataSource {
    /// Build an empty data source.
    pub fn new() -> Self {
        Self { inner: Arc::new(Mutex::new(InnerStore::default())) }
    }

    /// Build a data source pre-populated with sample accounts and messages.
    pub fn sample() -> Self {
        let me = Self::new();
        me.seed();
        me
    }

    fn seed(&self) {
        let now = Utc::now();
        let mut store = self.inner.lock().unwrap();

        let acc1 = make_account("Personal", "alice@example.com", 0);
        let acc2 = make_account("Work", "alice.j@corp.io", 1);
        let acc1_id = acc1.id;
        let acc2_id = acc2.id;

        let folders1 = make_folders(acc1_id);
        let folders2 = make_folders(acc2_id);

        let inbox1 = folders1[0].id;
        let inbox2 = folders2[0].id;

        let me1 = acc1.address.clone();
        let me2 = acc2.address.clone();

        let msgs1 = sample_messages(acc1_id, inbox1, &me1, now, 0);
        let msgs2 = sample_messages(acc2_id, inbox2, &me2, now, 100);

        for m in msgs1.iter().chain(msgs2.iter()) {
            if let Some(b) = m.body.clone() {
                store.bodies.insert(m.id, b);
            }
        }

        store.accounts.push(acc1);
        store.accounts.push(acc2);
        store.folders.insert(acc1_id, folders1);
        store.folders.insert(acc2_id, folders2);
        store.messages.insert(inbox1, msgs1);
        store.messages.insert(inbox2, msgs2);
    }
}

impl Default for InMemoryDataSource {
    fn default() -> Self {
        Self::new()
    }
}

impl DataSource for InMemoryDataSource {
    fn accounts(&self) -> Vec<Account> {
        let store = self.inner.lock().unwrap();
        let mut accs = store.accounts.clone();
        accs.sort_by_key(|a| a.order);
        accs
    }

    fn folders(&self, account: AccountId) -> Vec<Folder> {
        let store = self.inner.lock().unwrap();
        let mut folders = store.folders.get(&account).cloned().unwrap_or_default();
        folders.sort_by_key(|f| (folder_sort_key(f.role), f.name.to_lowercase()));
        folders
    }

    fn messages(&self, folder: FolderId) -> Vec<Message> {
        let store = self.inner.lock().unwrap();
        let mut msgs = store.messages.get(&folder).cloned().unwrap_or_default();
        msgs.sort_by(|a, b| b.internal_date.cmp(&a.internal_date));
        msgs
    }

    fn message_body(&self, message: MessageId) -> Option<MessageBody> {
        let store = self.inner.lock().unwrap();
        store.bodies.get(&message).cloned()
    }

    fn thread(&self, message: MessageId) -> Vec<Message> {
        let store = self.inner.lock().unwrap();
        let all: Vec<Message> = store.messages.values().flatten().cloned().collect();
        drop(store);
        crate::thread::collect_thread(&all, message)
    }

    fn save_draft(&self, draft: &Draft) -> anyhow::Result<()> {
        let mut store = self.inner.lock().unwrap();
        if let Some(existing) = store.drafts.iter_mut().find(|d| d.id == draft.id) {
            *existing = draft.clone();
        } else {
            store.drafts.push(draft.clone());
        }
        Ok(())
    }

    fn send(&self, draft: &Draft) -> anyhow::Result<()> {
        let mut store = self.inner.lock().unwrap();
        store.drafts.retain(|d| d.id != draft.id);
        Ok(())
    }

    fn mark_read(&self, message: MessageId) {
        self.set_seen(message, true);
    }

    fn set_seen(&self, message: MessageId, seen: bool) {
        let mut store = self.inner.lock().unwrap();
        for msgs in store.messages.values_mut() {
            for m in msgs.iter_mut() {
                if m.id == message {
                    let has = m.flags.contains(&Flag::Seen);
                    if seen && !has {
                        m.flags.push(Flag::Seen);
                    } else if !seen && has {
                        m.flags.retain(|f| f != &Flag::Seen);
                    }
                }
            }
        }
    }

    fn move_message(&self, message: MessageId, dest_folder: FolderId) -> anyhow::Result<()> {
        let mut store = self.inner.lock().unwrap();
        let mut moving: Option<imt_core::Message> = None;
        for msgs in store.messages.values_mut() {
            if let Some(pos) = msgs.iter().position(|m| m.id == message) {
                let mut m = msgs.remove(pos);
                m.folder_id = dest_folder;
                moving = Some(m);
                break;
            }
        }
        if let Some(m) = moving {
            store.messages.entry(dest_folder).or_default().push(m);
        }
        Ok(())
    }

    fn delete_message(&self, message: MessageId) -> anyhow::Result<()> {
        let trash_id: Option<FolderId> = {
            let store = self.inner.lock().unwrap();
            let acc = store.accounts.first().map(|a| a.id);
            acc.and_then(|aid| store.folders.get(&aid)
                .and_then(|fs| fs.iter().find(|f| f.role == FolderRole::Trash).map(|f| f.id)))
        };
        if let Some(dest) = trash_id {
            self.move_message(message, dest)
        } else {
            let mut store = self.inner.lock().unwrap();
            for msgs in store.messages.values_mut() {
                msgs.retain(|m| m.id != message);
            }
            Ok(())
        }
    }

    fn empty_trash(&self, folder: FolderId) -> anyhow::Result<()> {
        let mut store = self.inner.lock().unwrap();
        store.messages.insert(folder, Vec::new());
        if let Some(fs) = store.folders.values_mut().find(|fs| fs.iter().any(|f| f.id == folder)) {
            if let Some(f) = fs.iter_mut().find(|f| f.id == folder) {
                f.message_count = 0;
                f.unread_count = 0;
            }
        }
        Ok(())
    }

    fn toggle_flag(&self, message: MessageId) {
        let mut store = self.inner.lock().unwrap();
        for msgs in store.messages.values_mut() {
            for m in msgs.iter_mut() {
                if m.id == message {
                    if let Some(idx) = m.flags.iter().position(|f| matches!(f, Flag::Flagged)) {
                        m.flags.remove(idx);
                    } else {
                        m.flags.push(Flag::Flagged);
                    }
                }
            }
        }
    }

    fn add_account(&self, form: NewAccountForm) -> anyhow::Result<AccountId> {
        let mut store = self.inner.lock().unwrap();
        let order = store.accounts.len() as i32;
        let account = form.into_account(order);
        let id = account.id;
        let folders = make_folders(id);
        store.folders.insert(id, folders);
        store.accounts.push(account);
        Ok(id)
    }

    fn update_account(&self, id: AccountId, form: NewAccountForm, _pw_changed: bool) -> anyhow::Result<()> {
        let mut store = self.inner.lock().unwrap();
        let pos = store.accounts.iter().position(|a| a.id == id)
            .ok_or_else(|| anyhow::anyhow!("account not found"))?;
        let order = store.accounts[pos].order;
        let mut updated = form.into_account(order);
        updated.id = id;
        store.accounts[pos] = updated;
        Ok(())
    }

    fn delete_account(&self, id: AccountId) -> anyhow::Result<()> {
        let mut store = self.inner.lock().unwrap();
        store.accounts.retain(|a| a.id != id);
        store.folders.remove(&id);
        Ok(())
    }

    fn search(&self, query: &str) -> Vec<MessageId> {
        let q = query.to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        let store = self.inner.lock().unwrap();
        let mut out = Vec::new();
        for msgs in store.messages.values() {
            for m in msgs {
                let hay = format!(
                    "{} {} {}",
                    m.headers.subject,
                    m.headers.from.iter().map(|a| a.format()).collect::<Vec<_>>().join(" "),
                    m.snippet,
                );
                if hay.to_lowercase().contains(&q) {
                    out.push(m.id);
                }
            }
        }
        out
    }
}

fn folder_sort_key(role: FolderRole) -> u8 {
    match role {
        FolderRole::Inbox => 0,
        FolderRole::Other => 1,
        FolderRole::Archive => 2,
        FolderRole::Sent => 3,
        FolderRole::Junk => 4,
        FolderRole::Trash => 5,
        FolderRole::Drafts => 6,
    }
}

fn make_account(name: &str, email: &str, order: i32) -> Account {
    let id = AccountId::new();
    let auth = AuthMethod::Password { username: email.to_string() };
    Account {
        id,
        display_name: name.to_string(),
        address: Address::named(name, email),
        imap: ImapConfig {
            host: "imap.example.com".to_string(),
            port: 993,
            tls: Tls::Implicit,
            auth: auth.clone(),
        },
        smtp: SmtpConfig {
            host: "smtp.example.com".to_string(),
            port: 465,
            tls: Tls::Implicit,
            auth,
        },
        order,
    }
}

fn make_folders(account_id: AccountId) -> Vec<Folder> {
    let mk = |path: &str, name: &str, role: FolderRole, total: u32, unread: u32| Folder {
        id: FolderId::new(),
        account_id,
        path: path.to_string(),
        name: name.to_string(),
        role,
        uid_validity: 1,
        uid_next: total + 1,
        message_count: total,
        unread_count: unread,
    };
    vec![
        mk("INBOX", "Inbox", FolderRole::Inbox, 12, 4),
        mk("Sent", "Sent", FolderRole::Sent, 7, 0),
        mk("Drafts", "Drafts", FolderRole::Drafts, 2, 0),
        mk("Trash", "Trash", FolderRole::Trash, 0, 0),
        mk("Archive", "Archive", FolderRole::Archive, 34, 0),
    ]
}

fn sample_messages(
    account_id: AccountId,
    folder_id: FolderId,
    me: &Address,
    now: chrono::DateTime<Utc>,
    seed: u32,
) -> Vec<Message> {
    let entries: Vec<(&str, &str, &str, &str, bool, bool)> = vec![
        (
            "Bob Builder",
            "bob@build.io",
            "Re: project kickoff",
            "Sounds good. Let's meet Tuesday at 10. I'll bring the timeline draft so we can iterate together.",
            false,
            false,
        ),
        (
            "GitHub",
            "noreply@github.com",
            "[indicium/mail] PR #42 ready for review",
            "Carol has requested your review on PR #42 - 'Add IMAP idle support'.",
            true,
            false,
        ),
        (
            "Newsletter",
            "weekly@rust-lang.org",
            "This Week in Rust 528",
            "Hello and welcome to another issue of This Week in Rust!",
            false,
            false,
        ),
        (
            "Carol Chen",
            "carol@corp.io",
            "Lunch on Friday?",
            "Hey! Wanted to catch up. Are you free for lunch Friday around 12:30 at the usual place?",
            true,
            true,
        ),
        (
            "Dropbox",
            "no-reply@dropbox.com",
            "Your shared folder was updated",
            "Files were added to 'Designs/2026-Q2' by Eli.",
            false,
            false,
        ),
        (
            "Eli Park",
            "eli@studio.dev",
            "HTML mockup attached",
            "<html><body><h1>New brand</h1><p>Hi <b>there</b>,</p><p>Take a look at the attached mockups and let me know what you think.</p><ul><li>Color tokens</li><li>Type scale</li></ul></body></html>",
            true,
            false,
        ),
        (
            "Frank Ortiz",
            "frank@hosting.net",
            "Server maintenance window",
            "We'll be performing scheduled maintenance on Sunday between 02:00 and 04:00 UTC.",
            false,
            false,
        ),
        (
            "Gina Lee",
            "gina@finance.io",
            "Q1 expense report",
            "Attached is the Q1 expense report for your records. Let me know if anything looks off.",
            false,
            false,
        ),
        (
            "Heinrich M.",
            "heinrich@verein.de",
            "Vereinstreffen am Donnerstag",
            "Hallo zusammen, wir treffen uns am Donnerstag um 19 Uhr im Vereinsheim.",
            true,
            false,
        ),
        (
            "Ivy Stack",
            "ivy@notify.app",
            "You have 3 new mentions",
            "Three new mentions in #engineering. Tap to review them now.",
            false,
            false,
        ),
        (
            "Jules Romero",
            "jules@design.studio",
            "Contract for review",
            "Please find attached the latest version of the contract. Comments welcome.",
            false,
            true,
        ),
        (
            "Karen Cho",
            "karen@team.work",
            "Welcome to the team!",
            "We're excited to have you on board. Here are a few resources to get you started.",
            false,
            false,
        ),
    ];

    entries
        .into_iter()
        .enumerate()
        .map(|(i, (name, email, subject, body, unread, flagged))| {
            let hours_ago = (i as i64 + 1) * 7 + seed as i64;
            let date = now - Duration::hours(hours_ago);
            let from = Address::named(name, email);
            let is_html = body.starts_with("<html");
            let body_obj = if is_html {
                MessageBody {
                    text_plain: None,
                    text_html: Some(body.to_string()),
                    attachments: Vec::new(),
                }
            } else {
                MessageBody {
                    text_plain: Some(body.to_string()),
                    text_html: None,
                    attachments: Vec::new(),
                }
            };
            let snippet: String = body
                .chars()
                .filter(|c| !matches!(c, '<' | '>'))
                .take(120)
                .collect();
            let mut flags = Vec::new();
            if !unread {
                flags.push(Flag::Seen);
            }
            if flagged {
                flags.push(Flag::Flagged);
            }
            Message {
                id: MessageId::new(),
                account_id,
                folder_id,
                thread_id: None,
                uid: Uid((seed + i as u32 + 1) as u32),
                headers: MessageHeaders {
                    rfc_message_id: Some(format!("<sample-{}-{}@example.com>", seed, i)),
                    in_reply_to: None,
                    references: Vec::new(),
                    from: vec![from],
                    to: vec![me.clone()],
                    cc: Vec::new(),
                    bcc: Vec::new(),
                    reply_to: Vec::new(),
                    subject: subject.to_string(),
                    date,
                },
                flags,
                size: body.len() as u64,
                body: Some(body_obj),
                snippet,
                internal_date: date,
            }
        })
        .collect()
}

/// Helper: make a fresh empty `Draft` for a given account.
pub fn empty_draft(account_id: AccountId, from: Address) -> Draft {
    let now = Utc::now();
    Draft {
        id: DraftId::new(),
        account_id,
        kind: imt_core::draft::DraftKind::New,
        in_reply_to: None,
        from,
        to: Vec::new(),
        cc: Vec::new(),
        bcc: Vec::new(),
        subject: String::new(),
        body_text: String::new(),
        attachments: Vec::new(),
        created_at: now,
        updated_at: now,
    }
}
