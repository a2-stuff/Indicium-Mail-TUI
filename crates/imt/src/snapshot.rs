//! In-memory snapshot of mail state, refreshed from `SyncEvent`s and
//! initial loads from the store. Provides O(1) sync reads for the TUI.

use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::{Arc, Mutex, RwLock};

use anyhow::Result;
use imt_core::{
    Account, AccountId, Draft, Flag, Folder, FolderId, Message, MessageBody, MessageId, SyncEvent,
};
use imt_store::{AccountRepo, Db, DraftRepo, FolderRepo, MessageRepo};
use lru::LruCache;

/// Read-only snapshot of mail state, shared with the TUI.
#[derive(Default)]
pub struct SnapshotInner {
    pub accounts: Vec<Account>,
    pub folders_by_account: HashMap<AccountId, Vec<Folder>>,
    pub messages_by_folder: HashMap<FolderId, Vec<Message>>,
    pub drafts: Vec<Draft>,
    pub status: String,
    /// Pending toast notifications for the TUI to drain (errors, new mail, etc).
    pub notifications: VecDeque<String>,
}

/// Insert a message into a folder vector while preserving the
/// "newest first" ordering by `internal_date` (ties broken by id).
pub(crate) fn insert_message_sorted(vec: &mut Vec<Message>, m: Message) {
    let pos = vec
        .binary_search_by(|x| m.internal_date.cmp(&x.internal_date))
        .unwrap_or_else(|p| p);
    vec.insert(pos, m);
}

/// Cheap-clone handle to the snapshot.
#[derive(Clone)]
pub struct Snapshot {
    inner: Arc<RwLock<SnapshotInner>>,
    bodies: Arc<Mutex<LruCache<MessageId, MessageBody>>>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(SnapshotInner::default())),
            bodies: Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(1000).unwrap()))),
        }
    }
}

impl Snapshot {
    /// Construct an empty snapshot.
    pub fn new() -> Self {
        Self::default()
    }

    /// Look up a cached message body, if any.
    pub fn get_body(&self, id: MessageId) -> Option<MessageBody> {
        self.bodies.lock().unwrap().get(&id).cloned()
    }

    /// Insert a message body into the LRU cache.
    pub fn put_body(&self, id: MessageId, body: MessageBody) {
        self.bodies.lock().unwrap().put(id, body);
    }

    /// Read with a closure.
    pub fn read<R>(&self, f: impl FnOnce(&SnapshotInner) -> R) -> R {
        f(&*self.inner.read().unwrap())
    }

    /// Write with a closure.
    pub fn write<R>(&self, f: impl FnOnce(&mut SnapshotInner) -> R) -> R {
        f(&mut *self.inner.write().unwrap())
    }

    /// Set the status line text.
    pub fn set_status(&self, text: impl Into<String>) {
        self.write(|s| s.status = text.into());
    }

    /// Queue a toast notification (errors, new mail, etc).
    pub fn push_notification(&self, text: impl Into<String>) {
        self.write(|s| s.notifications.push_back(text.into()));
    }

    /// Drain the next pending notification, if any.
    pub fn pop_notification(&self) -> Option<String> {
        self.write(|s| s.notifications.pop_front())
    }

    /// Hydrate the snapshot from the database. Loads all accounts, folders,
    /// and recent messages per folder.
    pub async fn hydrate_from_db(&self, db: &Db) -> Result<()> {
        let pool = db.pool();
        let accounts = AccountRepo::new(pool).list().await?;
        let mut folders_by_account = HashMap::new();
        let mut messages_by_folder = HashMap::new();
        for acc in &accounts {
            let folders = FolderRepo::new(pool).list_by_account(acc.id).await?;
            for f in &folders {
                let msgs = MessageRepo::new(pool).list_by_folder(f.id, 500, 0).await?;
                messages_by_folder.insert(f.id, msgs);
            }
            folders_by_account.insert(acc.id, folders);
        }
        let drafts = match accounts.first() {
            Some(a) => DraftRepo::new(pool).list_by_account(a.id).await?,
            None => Vec::new(),
        };
        self.write(|s| {
            s.accounts = accounts;
            s.folders_by_account = folders_by_account;
            s.messages_by_folder = messages_by_folder;
            s.drafts = drafts;
        });
        Ok(())
    }

    /// Apply a single sync event to the snapshot, refreshing affected rows
    /// from the database.
    pub async fn apply_event(&self, db: &Db, event: &SyncEvent) -> Result<()> {
        let pool = db.pool();
        match event {
            SyncEvent::AccountConnecting { .. } => self.set_status("connecting"),
            SyncEvent::AccountConnected { .. } => self.set_status("connected"),
            SyncEvent::AccountDisconnected { reason, .. } => {
                self.set_status(format!("disconnected: {}", reason))
            }
            SyncEvent::Error { message, .. } => {
                self.push_notification(format!("Error: {}", message));
                self.set_status(String::new());
            }
            SyncEvent::SyncStarted { .. } => self.set_status("syncing"),
            SyncEvent::SyncFinished { .. } => self.set_status("idle"),
            SyncEvent::FolderListUpdated { account_id } => {
                let folders = FolderRepo::new(pool).list_by_account(*account_id).await?;
                self.write(|s| {
                    s.folders_by_account.insert(*account_id, folders);
                });
            }
            SyncEvent::FolderCountsChanged { folder_id, total, unread } => {
                self.write(|s| {
                    for fs in s.folders_by_account.values_mut() {
                        if let Some(f) = fs.iter_mut().find(|f| f.id == *folder_id) {
                            f.message_count = *total;
                            f.unread_count = *unread;
                        }
                    }
                });
            }
            SyncEvent::MessageAdded { folder_id, message_id } => {
                let msgs = MessageRepo::new(pool).list_by_folder(*folder_id, 500, 0).await?;
                // Notify if this is an inbox folder and the message is new (not Seen).
                if let Some(msg) = msgs.iter().find(|m| m.id == *message_id) {
                    let is_inbox = self.read(|s| {
                        s.folders_by_account.values().any(|fs| {
                            fs.iter().any(|f| f.id == *folder_id && f.role == imt_core::FolderRole::Inbox)
                        })
                    });
                    if is_inbox && !msg.flags.contains(&imt_core::Flag::Seen) {
                        let from = msg.headers.from.first()
                            .map(|a| a.format())
                            .unwrap_or_else(|| "Unknown".into());
                        self.push_notification(format!("New mail from {}:\n{}", from, msg.headers.subject));
                    }
                }
                self.write(|s| {
                    s.messages_by_folder.insert(*folder_id, msgs);
                });
            }
            SyncEvent::MessageRemoved { folder_id, .. } => {
                let msgs = MessageRepo::new(pool).list_by_folder(*folder_id, 500, 0).await?;
                self.write(|s| {
                    s.messages_by_folder.insert(*folder_id, msgs);
                });
            }
            SyncEvent::MessageFlagsChanged { message_id, flags } => {
                let flags = flags.clone();
                self.write(|s| {
                    for msgs in s.messages_by_folder.values_mut() {
                        if let Some(m) = msgs.iter_mut().find(|m| m.id == *message_id) {
                            m.flags = flags.clone();
                        }
                    }
                });
            }
            SyncEvent::MessageBodyFetched { message_id } => {
                let msg = MessageRepo::new(pool).get(*message_id).await?;
                if let Some(body) = msg.body.clone() {
                    self.put_body(*message_id, body);
                }
            }
        }
        Ok(())
    }

    /// Mark a message read locally (idempotent).
    pub fn mark_local_read(&self, message_id: MessageId) {
        self.write(|s| {
            for msgs in s.messages_by_folder.values_mut() {
                if let Some(m) = msgs.iter_mut().find(|m| m.id == message_id) {
                    if !m.flags.contains(&Flag::Seen) {
                        m.flags.push(Flag::Seen);
                    }
                }
            }
        });
    }

    /// Add an account to the snapshot immediately (engine will persist async).
    pub fn add_local_account(&self, account: Account) {
        self.write(|s| {
            s.accounts.push(account);
            s.accounts.sort_by_key(|a| a.order);
        });
    }
}
