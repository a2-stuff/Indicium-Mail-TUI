//! Public sync engine: owns per-account workers and exposes commands.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
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
use crate::password::{imap_provider_for, smtp_provider_for};

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

    /// Persist a new account, store its IMAP password in the keyring, and spawn a worker.
    pub async fn add_account(&self, account: Account, password: String) -> Result<()> {
        AccountRepo::new(self.db.pool()).upsert(&account).await?;
        secrets::store(account.id, "imap_password", &password);
        secrets::store(account.id, "smtp_password", &password);
        self.spawn_task(account).await;
        Ok(())
    }

    /// Cancel an account's worker (if any), delete its row from the store, and clear secrets.
    pub async fn remove_account(&self, id: AccountId) -> Result<()> {
        if let Some(task) = self.tasks.lock().await.remove(&id) {
            task.cancel.notify_waiters();
            let _ = task.handle.await;
        }
        secrets::delete(id, "imap_password");
        secrets::delete(id, "smtp_password");
        AccountRepo::new(self.db.pool()).delete(id).await?;
        Ok(())
    }

    /// Trigger an explicit one-shot sync for `(account, folder)`.
    pub async fn sync_folder(&self, account: AccountId, folder_id: FolderId) -> Result<()> {
        let acc = AccountRepo::new(self.db.pool()).get(account).await?;
        let folder = FolderRepo::new(self.db.pool()).get(folder_id).await?;

        let provider = imap_provider_for(account);
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

        let provider = imap_provider_for(acc.id);
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

        let smtp = SmtpSender::new(acc.clone(), smtp_provider_for(acc.id));
        smtp.send(&draft.from, &draft.to, &draft.cc, &draft.bcc, &raw)
            .await?;

        let sent_folder = find_sent_folder(&self.db, acc.id).await;
        if let Some(sent) = sent_folder {
            let provider = imap_provider_for(acc.id);
            let mut backend = ImapBackend::new(acc.clone(), provider);
            match backend.connect().await {
                Ok(()) => {
                    if let Err(e) = backend.append(&sent.path, &raw, &[Flag::Seen]).await {
                        warn!(target: "imt-sync::engine", "append to Sent failed: {}", e);
                    } else {
                        let stub = Message {
                            id: MessageId::new(),
                            account_id: acc.id,
                            folder_id: sent.id,
                            thread_id: None,
                            uid: Uid(0),
                            headers: imt_core::MessageHeaders {
                                rfc_message_id: None,
                                in_reply_to: None,
                                references: Vec::new(),
                                from: vec![draft.from.clone()],
                                to: draft.to.clone(),
                                cc: draft.cc.clone(),
                                bcc: draft.bcc.clone(),
                                reply_to: Vec::new(),
                                subject: draft.subject.clone(),
                                date: Utc::now(),
                            },
                            flags: vec![Flag::Seen],
                            size: raw.len() as u64,
                            body: None,
                            snippet: crate::snippet::make_snippet(&draft.body_text, 256),
                            internal_date: Utc::now(),
                        };
                        if let Err(e) = MessageRepo::new(self.db.pool())
                            .upsert_envelope(&stub)
                            .await
                        {
                            warn!(target: "imt-sync::engine", "store sent stub: {}", e);
                        }
                    }
                    let _ = backend.disconnect().await;
                }
                Err(e) => warn!(target: "imt-sync::engine", "append connect failed: {}", e),
            }
        }

        DraftRepo::new(self.db.pool()).delete(draft.id).await?;
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

#[allow(dead_code)]
fn _unused_keep(_: &Address) {}
