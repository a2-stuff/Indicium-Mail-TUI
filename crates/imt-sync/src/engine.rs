//! Public sync engine: owns per-account workers and exposes commands.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{mpsc, Mutex, Notify};
use tokio::task::JoinHandle;
use tracing::warn;

use imt_core::{
    Account, AccountId, Address, Draft, Flag, FolderId, FolderRole, Message, MessageBody,
    MessageId, SyncEvent, Uid,
};
use imt_net::backend::MailBackend;
use imt_net::smtp::{BuildDraft, DraftAttachmentRef};
use imt_net::{build_rfc822, ImapBackend, SmtpSender};
use imt_store::{secrets, AccountRepo, Db, DraftRepo, FolderRepo, MessageRepo};

use crate::account_task::{run as run_account_task, AccountTaskCtx};
use crate::error::{Result, SyncError};
use crate::password::{delete_all, imap_provider_for, smtp_provider_for, store_password};

/// OAuth2 code exchange info passed to `add_account` when setting up a new
/// OAuth2 account. The engine exchanges the authorization code for tokens and
/// stores them in the secrets store.
pub struct OAuthExchange {
    pub client_id: String,
    pub client_secret: String,
    pub code: String,
    pub verifier: String,
    pub redirect_uri: String,
}

/// Handle for a running per-account worker.
struct AccountTask {
    cancel: Arc<Notify>,
    handle: JoinHandle<()>,
}

/// Sync engine: owns DB, event channel, and per-account worker tasks.
pub struct SyncEngine {
    db: Arc<Db>,
    tx: mpsc::UnboundedSender<SyncEvent>,
    tasks: Arc<Mutex<HashMap<AccountId, AccountTask>>>,
}

impl SyncEngine {
    /// Construct a new engine and return it together with the event receiver.
    pub fn new(db: Arc<Db>) -> (Self, mpsc::UnboundedReceiver<SyncEvent>) {
        let (tx, rx) = mpsc::unbounded_channel();
        let engine = Self {
            db,
            tx,
            tasks: Arc::new(Mutex::new(HashMap::new())),
        };
        (engine, rx)
    }

    /// Persist a new account. For password accounts, stores the password in secrets.
    /// For OAuth2 accounts, exchanges the authorization code for tokens and stores them.
    pub async fn add_account(
        &self,
        account: Account,
        password: String,
        oauth_exchange: Option<OAuthExchange>,
    ) -> Result<()> {
        AccountRepo::new(self.db.pool()).upsert(&account).await?;
        if let Some(ex) = oauth_exchange {
            let provider = imt_net::OAuthProvider::from_imap_host(&account.imap.host)
                .unwrap_or(imt_net::OAuthProvider::Google);
            let client_secret = if ex.client_secret.is_empty() { None } else { Some(ex.client_secret.clone()) };
            let flow = imt_net::OAuthFlow::new(provider, ex.client_id.clone(), client_secret.clone());
            let verifier = imt_net::PkceVerifier(ex.verifier);
            let tokens = flow.exchange_code(&ex.code, verifier, &ex.redirect_uri).await
                .map_err(|e| crate::error::SyncError::Other(format!("OAuth2 code exchange: {e}")))?;
            secrets::store(account.id, "oauth_access_token", &tokens.access_token);
            secrets::store(account.id, "oauth_access_expiry", &tokens.expires_at.timestamp().to_string());
            if let Some(rt) = &tokens.refresh_token {
                secrets::store(account.id, "oauth_refresh_token", rt);
            }
            if let Some(cs) = &client_secret {
                secrets::store(account.id, "oauth_client_secret", cs);
            }
        } else {
            store_password(account.id, &password);
        }
        self.spawn_task(account).await;
        Ok(())
    }

    /// Update an existing account. If `password` is `Some`, the new value
    /// is stored in the keyring/file. Restarts the per-account worker.
    pub async fn update_account(&self, account: Account, password: Option<String>) -> Result<()> {
        AccountRepo::new(self.db.pool()).upsert(&account).await?;
        if let Some(p) = password {
            secrets::store(account.id, "imap_password", &p);
            secrets::store(account.id, "smtp_password", &p);
        }
        if let Some(prev) = self.tasks.lock().await.remove(&account.id) {
            prev.cancel.notify_waiters();
            let _ = prev.handle.await;
        }
        self.spawn_task(account).await;
        Ok(())
    }

    /// Cancel an account's worker (if any), delete its row from the store, and clear secrets.
    pub async fn remove_account(&self, id: AccountId) -> Result<()> {
        if let Some(task) = self.tasks.lock().await.remove(&id) {
            task.cancel.notify_waiters();
            let _ = task.handle.await;
        }
        delete_all(id);
        AccountRepo::new(self.db.pool()).delete(id).await?;
        Ok(())
    }

    /// Trigger an explicit one-shot sync for `(account, folder)`.
    pub async fn sync_folder(&self, account: AccountId, folder_id: FolderId) -> Result<()> {
        let acc = AccountRepo::new(self.db.pool()).get(account).await?;
        let folder = FolderRepo::new(self.db.pool()).get(folder_id).await?;

        let provider = imap_provider_for(&acc);
        let mut backend = ImapBackend::new(acc.clone(), provider);
        backend.connect().await?;

        let _ = self.tx.send(SyncEvent::SyncStarted {
            account_id: account,
            folder_id: Some(folder_id),
        });

        let state = backend.select_folder(&folder.path).await?;
        let last_uid_next = folder.uid_next;
        let range = if last_uid_next == 0 || state.uid_next <= last_uid_next {
            imt_net::backend::UidRange::All
        } else {
            imt_net::backend::UidRange::Range(
                last_uid_next,
                state.uid_next.saturating_sub(1).max(last_uid_next),
            )
        };

        let envelopes = backend.fetch_envelopes(&folder.path, range).await?;
        let msg_repo = MessageRepo::new(self.db.pool());
        for env in envelopes {
            let existing = msg_repo
                .get_by_uid(folder_id, Uid(env.uid))
                .await
                .ok();
            let message = match existing {
                Some(mut m) => {
                    m.flags = env.flags.clone();
                    m.headers = env.headers.clone();
                    m.size = env.size;
                    m.internal_date = env.internal_date;
                    // Keep a known-true (from a fetched body) but let the header
                    // heuristic light it up before the body is fetched.
                    m.has_attachments = m.has_attachments || env.has_attachments;
                    m
                }
                None => Message {
                    id: MessageId::new(),
                    account_id: account,
                    folder_id,
                    thread_id: None,
                    uid: Uid(env.uid),
                    snippet: crate::snippet::make_snippet(&env.headers.subject, 256),
                    headers: env.headers,
                    flags: env.flags,
                    size: env.size,
                    body: None,
                    has_attachments: env.has_attachments,
                    internal_date: env.internal_date,
                },
            };
            let id = message.id;
            msg_repo.upsert_envelope(&message).await?;
            let _ = self.tx.send(SyncEvent::MessageAdded {
                folder_id,
                message_id: id,
            });
        }

        let folder_repo = FolderRepo::new(self.db.pool());
        let updated = imt_core::Folder {
            uid_validity: state.uid_validity,
            uid_next: state.uid_next,
            message_count: state.exists,
            unread_count: state.unseen,
            ..folder
        };
        folder_repo.upsert(&updated).await?;
        let _ = self.tx.send(SyncEvent::FolderCountsChanged {
            folder_id,
            total: state.exists,
            unread: state.unseen,
        });
        let _ = self.tx.send(SyncEvent::SyncFinished {
            account_id: account,
            folder_id: Some(folder_id),
        });
        let _ = backend.disconnect().await;
        Ok(())
    }

    /// Fetch the body for `message_id`, persist it, and return it.
    pub async fn fetch_body(&self, message_id: MessageId) -> Result<MessageBody> {
        let msg_repo = MessageRepo::new(self.db.pool());
        let msg = msg_repo.get(message_id).await?;
        let folder = FolderRepo::new(self.db.pool()).get(msg.folder_id).await?;
        let acc = AccountRepo::new(self.db.pool()).get(msg.account_id).await?;

        let provider = imap_provider_for(&acc);
        let mut backend = ImapBackend::new(acc, provider);
        backend.connect().await?;
        let body = backend.fetch_body(&folder.path, msg.uid.0).await?;
        msg_repo.set_body(message_id, &body).await?;
        let _ = self.tx.send(SyncEvent::MessageBodyFetched { message_id });
        let _ = backend.disconnect().await;
        Ok(body)
    }

    /// Send a draft via SMTP, append a copy to the account's Sent folder, and mark it sent.
    pub async fn send(&self, draft: &Draft) -> Result<()> {
        let acc = AccountRepo::new(self.db.pool())
            .get(draft.account_id)
            .await?;

        let attachments = load_attachments(&draft.attachments).await?;
        let in_reply_to_header = match draft.in_reply_to {
            Some(mid) => MessageRepo::new(self.db.pool())
                .get(mid)
                .await
                .ok()
                .and_then(|m| m.headers.rfc_message_id),
            None => None,
        };
        let references: Vec<String> = match draft.in_reply_to {
            Some(mid) => MessageRepo::new(self.db.pool())
                .get(mid)
                .await
                .ok()
                .map(|m| {
                    let mut r = m.headers.references.clone();
                    if let Some(rid) = m.headers.rfc_message_id.clone() {
                        if !r.contains(&rid) {
                            r.push(rid);
                        }
                    }
                    r
                })
                .unwrap_or_default(),
            None => Vec::new(),
        };

        let build = BuildDraft {
            from: draft.from.clone(),
            to: draft.to.clone(),
            cc: draft.cc.clone(),
            bcc: draft.bcc.clone(),
            subject: draft.subject.clone(),
            body_text: draft.body_text.clone(),
            attachments,
            in_reply_to: in_reply_to_header,
            references,
        };
        let raw = build_rfc822(&build)?;

        let smtp = SmtpSender::new(acc.clone(), smtp_provider_for(&acc));
        smtp.send(&draft.from, &draft.to, &draft.cc, &draft.bcc, &raw)
            .await?;

        let sent_folder = find_sent_folder(&self.db, acc.id).await;
        if let Some(sent) = sent_folder {
            let provider = imap_provider_for(&acc);
            let mut backend = ImapBackend::new(acc.clone(), provider);
            match backend.connect().await {
                Ok(()) => {
                    if let Err(e) = backend.append(&sent.path, &raw, &[Flag::Seen]).await {
                        warn!(target: "imt-sync::engine", "append to Sent failed: {}", e);
                    }
                    // Trigger a sync so the new sent message appears.
                    let _ = self.tx.send(SyncEvent::SyncStarted {
                        account_id: acc.id,
                        folder_id: Some(sent.id),
                    });
                    let _ = backend.disconnect().await;
                }
                Err(e) => warn!(target: "imt-sync::engine", "append connect failed: {}", e),
            }
        }

        DraftRepo::new(self.db.pool()).delete(draft.id).await?;
        Ok(())
    }

    /// Move a message to another folder. Persists the move locally as well.
    pub async fn move_message(&self, message_id: MessageId, dest_folder_id: FolderId) -> Result<()> {
        let msg_repo = MessageRepo::new(self.db.pool());
        let folder_repo = FolderRepo::new(self.db.pool());
        let msg = msg_repo.get(message_id).await?;
        let src_folder = folder_repo.get(msg.folder_id).await?;
        let dst_folder = folder_repo.get(dest_folder_id).await?;
        let acc = AccountRepo::new(self.db.pool()).get(msg.account_id).await?;
        let was_unread = !msg.flags.contains(&Flag::Seen);

        let provider = imap_provider_for(&acc);
        let mut backend = ImapBackend::new(acc, provider);
        backend.connect().await?;
        backend.move_uid(&src_folder.path, msg.uid.0, &dst_folder.path).await?;
        let _ = backend.disconnect().await;

        if let Err(e) = msg_repo.delete_by_uid(src_folder.id, msg.uid).await {
            tracing::warn!(target: "imt-sync::engine", "delete_by_uid after move failed: {} - scheduling resync", e);
            // Queue a background resync to reconcile state. We can't do it inline
            // without recursion concerns, so just log and let next IDLE/refresh handle it.
            let _ = self.tx.send(SyncEvent::SyncFinished { account_id: msg.account_id, folder_id: Some(src_folder.id) });
            return Err(e.into());
        }
        let _ = self.tx.send(SyncEvent::MessageRemoved {
            folder_id: src_folder.id,
            uid: msg.uid,
        });

        // Refresh stored counts so the sidebar reflects the move immediately,
        // even for folders the user hasn't opened yet.
        if let Ok((src_total, src_unread)) = count_folder(&msg_repo, src_folder.id).await {
            let _ = folder_repo.update_counts(src_folder.id, src_total, src_unread).await;
            let _ = self.tx.send(SyncEvent::FolderCountsChanged {
                folder_id: src_folder.id,
                total: src_total,
                unread: src_unread,
            });
        }
        if let Ok((dst_total_db, dst_unread_db)) = count_folder(&msg_repo, dst_folder.id).await {
            // The moved message isn't in the dst DB yet (IMAP MOVE happened
            // server-side; we resync the folder later). Add the in-flight one.
            let dst_total = dst_total_db.saturating_add(1);
            let dst_unread = if was_unread { dst_unread_db.saturating_add(1) } else { dst_unread_db };
            let _ = folder_repo.update_counts(dst_folder.id, dst_total, dst_unread).await;
            let _ = self.tx.send(SyncEvent::FolderCountsChanged {
                folder_id: dst_folder.id,
                total: dst_total,
                unread: dst_unread,
            });
        }
        Ok(())
    }

    /// Hard-delete every message in `folder` by issuing IMAP `STORE \Deleted`
    /// for all UIDs and EXPUNGE-ing. Clears local state too. Intended for the
    /// Trash folder; the caller is responsible for that policy.
    pub async fn empty_trash(&self, folder_id: FolderId) -> Result<()> {
        let msg_repo = MessageRepo::new(self.db.pool());
        let folder_repo = FolderRepo::new(self.db.pool());
        let folder = folder_repo.get(folder_id).await?;
        let acc = AccountRepo::new(self.db.pool()).get(folder.account_id).await?;

        let provider = imap_provider_for(&acc);
        let mut backend = ImapBackend::new(acc, provider);
        backend.connect().await?;
        backend.expunge_folder(&folder.path).await?;
        let _ = backend.disconnect().await;

        msg_repo.delete_by_folder(folder_id).await?;
        let _ = folder_repo.update_counts(folder_id, 0, 0).await;
        let _ = self.tx.send(SyncEvent::FolderCountsChanged {
            folder_id,
            total: 0,
            unread: 0,
        });
        Ok(())
    }

    /// Set or clear a flag on a message. Contacts the IMAP server, updates the
    /// DB, and emits `MessageFlagsChanged` so the snapshot reflects the change.
    pub async fn set_flag(&self, message_id: MessageId, flag: Flag, add: bool) -> Result<()> {
        let msg_repo = MessageRepo::new(self.db.pool());
        let msg = msg_repo.get(message_id).await?;
        let folder = FolderRepo::new(self.db.pool()).get(msg.folder_id).await?;
        let acc = AccountRepo::new(self.db.pool()).get(msg.account_id).await?;

        let provider = imap_provider_for(&acc);
        let mut backend = ImapBackend::new(acc, provider);
        backend.connect().await?;

        let (add_v, rem_v): (Vec<Flag>, Vec<Flag>) = if add {
            (vec![flag.clone()], Vec::new())
        } else {
            (Vec::new(), vec![flag.clone()])
        };
        backend.set_flags(&folder.path, msg.uid.0, &add_v, &rem_v).await?;
        let _ = backend.disconnect().await;

        let mut updated_flags = msg.flags.clone();
        if add {
            if !updated_flags.contains(&flag) {
                updated_flags.push(flag);
            }
        } else {
            updated_flags.retain(|f| f != &flag);
        }
        let mut updated_msg = msg;
        updated_msg.flags = updated_flags.clone();
        msg_repo.upsert_envelope(&updated_msg).await?;

        let _ = self.tx.send(SyncEvent::MessageFlagsChanged {
            message_id,
            flags: updated_flags,
        });
        Ok(())
    }

    /// Build the RFC 822 for a draft and APPEND it to the account's Drafts folder.
    pub async fn save_draft_to_imap(&self, draft: &Draft) -> Result<()> {
        let acc = AccountRepo::new(self.db.pool()).get(draft.account_id).await?;
        let attachments = load_attachments(&draft.attachments).await?;
        let build = BuildDraft {
            from: draft.from.clone(),
            to: draft.to.clone(),
            cc: draft.cc.clone(),
            bcc: draft.bcc.clone(),
            subject: draft.subject.clone(),
            body_text: draft.body_text.clone(),
            attachments,
            in_reply_to: None,
            references: Vec::new(),
        };
        let raw = build_rfc822(&build)?;

        let folders = FolderRepo::new(self.db.pool()).list_by_account(acc.id).await?;
        let drafts_folder = folders.iter()
            .find(|f| f.role == FolderRole::Drafts)
            .or_else(|| folders.iter().find(|f| f.path.eq_ignore_ascii_case("Drafts")));

        if let Some(folder) = drafts_folder {
            let provider = imap_provider_for(&acc);
            let mut backend = ImapBackend::new(acc.clone(), provider);
            match backend.connect().await {
                Ok(()) => {
                    if let Err(e) = backend.append(&folder.path, &raw, &[Flag::Seen]).await {
                        warn!(target: "imt-sync::engine", "append to Drafts failed: {}", e);
                    }
                    let _ = backend.disconnect().await;
                    // Trigger a sync of the Drafts folder so it appears in the list.
                    let _ = self.tx.send(SyncEvent::SyncStarted { account_id: acc.id, folder_id: Some(folder.id) });
                }
                Err(e) => warn!(target: "imt-sync::engine", "draft append connect failed: {}", e),
            }
        }
        Ok(())
    }

    /// Cancel every running worker and wait for them to exit.
    pub async fn shutdown(&self) -> Result<()> {
        let mut tasks = self.tasks.lock().await;
        for (_, t) in tasks.drain() {
            t.cancel.notify_waiters();
            let _ = t.handle.await;
        }
        Ok(())
    }

    async fn spawn_task(&self, account: Account) {
        let cancel = Arc::new(Notify::new());
        let ctx = AccountTaskCtx {
            db: Arc::clone(&self.db),
            tx: self.tx.clone(),
            cancel: Arc::clone(&cancel),
            account: account.clone(),
        };
        let id = account.id;
        let handle = tokio::spawn(async move { run_account_task(ctx).await });
        let mut tasks = self.tasks.lock().await;
        if let Some(prev) = tasks.insert(
            id,
            AccountTask {
                cancel: Arc::clone(&cancel),
                handle,
            },
        ) {
            prev.cancel.notify_waiters();
            let _ = prev.handle.await;
        }
    }
}

async fn load_attachments(
    drafts: &[imt_core::draft::DraftAttachment],
) -> Result<Vec<DraftAttachmentRef>> {
    let mut out = Vec::with_capacity(drafts.len());
    for a in drafts {
        let bytes = tokio::fs::read(&a.path)
            .await
            .map_err(|e| SyncError::Other(format!("read attachment {}: {}", a.filename, e)))?;
        out.push(DraftAttachmentRef {
            filename: a.filename.clone(),
            mime: a.mime_type.clone(),
            bytes,
        });
    }
    Ok(out)
}

async fn find_sent_folder(db: &Db, account_id: AccountId) -> Option<imt_core::Folder> {
    let repo = FolderRepo::new(db.pool());
    let folders = repo.list_by_account(account_id).await.ok()?;
    folders
        .iter()
        .find(|f| f.role == FolderRole::Sent)
        .or_else(|| {
            folders
                .iter()
                .find(|f| f.path.eq_ignore_ascii_case("Sent") || f.path.contains("Sent"))
        })
        .cloned()
}

/// Count total and unread messages currently stored locally for a folder.
async fn count_folder(repo: &MessageRepo<'_>, folder_id: FolderId) -> Result<(u32, u32)> {
    // 10_000 is well above any folder we expect to display; if it ever isn't,
    // the next full sync will reset counts authoritatively from the server.
    let msgs = repo.list_by_folder(folder_id, 10_000, 0).await?;
    let total = msgs.len() as u32;
    let unread = msgs.iter().filter(|m| !m.flags.contains(&Flag::Seen)).count() as u32;
    Ok((total, unread))
}

#[allow(dead_code)]
fn _unused_keep(_: &Address) {}
