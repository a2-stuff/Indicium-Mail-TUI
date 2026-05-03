//! Per-account async worker loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::select;
use tokio::sync::{mpsc, Notify};
use tracing::{debug, error, info, warn};

use imt_core::{
    Account, AccountId, Address, Flag, Folder, FolderId, FolderRole, Message, MessageHeaders,
    MessageId, SyncEvent, Uid,
};
use imt_net::backend::{EnvelopeFetch, FolderInfo, IdleEvent, MailBackend, UidRange};
use imt_net::ImapBackend;
use imt_store::{Db, FolderRepo, MessageRepo};

use crate::password::imap_provider_for;
use crate::snippet::make_snippet;

const SNIPPET_MAX: usize = 256;
const BACKOFF_INITIAL_SECS: u64 = 5;
const BACKOFF_MAX_SECS: u64 = 300;

/// Convert `imt_net::FolderInfo` into `imt_core::Folder`, preserving any
/// existing `uid_next` / counts so we don't blindly trust the LIST
/// response and skip a needed envelope fetch.
fn to_folder(
    account_id: AccountId,
    id: FolderId,
    info: &FolderInfo,
    existing: Option<&Folder>,
) -> Folder {
    let (uid_next, message_count, unread_count) = match existing {
        Some(f) => (f.uid_next, f.message_count, f.unread_count),
        None => (0, 0, 0),
    };
    Folder {
        id,
        account_id,
        path: info.path.clone(),
        name: info.name.clone(),
        role: info.role,
        uid_validity: info.uid_validity,
        uid_next,
        message_count,
        unread_count,
    }
}

/// Convert an envelope fetch into a fresh `Message` (new MessageId).
fn to_message(account_id: AccountId, folder_id: FolderId, env: EnvelopeFetch) -> Message {
    let snippet = if env.snippet.is_empty() {
        make_snippet(&env.headers.subject, SNIPPET_MAX)
    } else {
        make_snippet(&env.snippet, SNIPPET_MAX)
    };
    Message {
        id: MessageId::new(),
        account_id,
        folder_id,
        thread_id: None,
        uid: Uid(env.uid),
        headers: env.headers,
        flags: env.flags,
        size: env.size,
        body: None,
        snippet,
        internal_date: env.internal_date,
    }
}

/// Configuration for a spawned account worker.
pub struct AccountTaskCtx {
    /// Database handle.
    pub db: Arc<Db>,
    /// Sink for `SyncEvent`s.
    pub tx: mpsc::UnboundedSender<SyncEvent>,
    /// Cancellation signal.
    pub cancel: Arc<Notify>,
    /// Account being synchronised.
    pub account: Account,
}

/// Run the per-account loop until cancelled.
pub async fn run(ctx: AccountTaskCtx) {
    let mut backoff = BACKOFF_INITIAL_SECS;
    loop {
        let result = run_once(&ctx).await;
        match result {
            Ok(()) => return,
            Err(SyncErrorReason::Cancelled) => return,
            Err(SyncErrorReason::Other(msg)) => {
                error!(target: "imt-sync::account_task", account = ?ctx.account.id, "{}", msg);
                let _ = ctx.tx.send(SyncEvent::Error {
                    account_id: Some(ctx.account.id),
                    message: msg.clone(),
                });
                let _ = ctx.tx.send(SyncEvent::AccountDisconnected {
                    account_id: ctx.account.id,
                    reason: msg,
                });
                let sleep = Duration::from_secs(backoff);
                select! {
                    _ = ctx.cancel.notified() => return,
                    _ = tokio::time::sleep(sleep) => {}
                }
                backoff = (backoff * 2).min(BACKOFF_MAX_SECS);
            }
        }
    }
}

enum SyncErrorReason {
    Cancelled,
    Other(String),
}

impl std::fmt::Display for SyncErrorReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SyncErrorReason::Cancelled => f.write_str("cancelled"),
            SyncErrorReason::Other(s) => f.write_str(s),
        }
    }
}

async fn run_once(ctx: &AccountTaskCtx) -> std::result::Result<(), SyncErrorReason> {
    let _ = ctx.tx.send(SyncEvent::AccountConnecting {
        account_id: ctx.account.id,
    });

    let provider = imap_provider_for(ctx.account.id);
    let mut backend = ImapBackend::new(ctx.account.clone(), provider);
    backend
        .connect()
        .await
        .map_err(|e| SyncErrorReason::Other(format!("connect: {}", e)))?;

    let _ = ctx.tx.send(SyncEvent::AccountConnected {
        account_id: ctx.account.id,
    });

    let folders = sync_folder_list(ctx, &mut backend).await?;

    for f in &folders {
        if ctx.cancel.notified_now() {
            return Err(SyncErrorReason::Cancelled);
        }
        if let Err(e) = sync_one_folder(ctx, &mut backend, f).await {
            warn!(target: "imt-sync::account_task", "folder sync {}: {}", f.path, e);
            let _ = ctx.tx.send(SyncEvent::Error {
                account_id: Some(ctx.account.id),
                message: format!("folder {}: {}", f.path, e),
            });
        }
    }

    let inbox_path = folders
        .iter()
        .find(|f| f.role == FolderRole::Inbox)
        .map(|f| f.path.clone())
        .or_else(|| folders.first().map(|f| f.path.clone()));

    let inbox_path = match inbox_path {
        Some(p) => p,
        None => {
            select! {
                _ = ctx.cancel.notified() => return Err(SyncErrorReason::Cancelled),
                _ = tokio::time::sleep(Duration::from_secs(60)) => return Ok(()),
            }
        }
    };

    info!(target: "imt-sync::account_task", "entering idle on {}", inbox_path);
    let mut idle = backend
        .idle(&inbox_path)
        .await
        .map_err(|e| SyncErrorReason::Other(format!("idle: {}", e)))?;

    loop {
        select! {
            _ = ctx.cancel.notified() => {
                let _ = idle.done().await;
                return Err(SyncErrorReason::Cancelled);
            }
            ev = idle.next() => {
                match ev {
                    Ok(IdleEvent::Exists(_)) | Ok(IdleEvent::Expunge(_)) | Ok(IdleEvent::Flags(_)) => {
                        let _ = idle.done().await;
                        let folders = sync_folder_list(ctx, &mut backend).await?;
                        if let Some(folder) = folders.iter().find(|f| f.path == inbox_path) {
                            if let Err(e) = sync_one_folder(ctx, &mut backend, folder).await {
                                let _ = ctx.tx.send(SyncEvent::Error {
                                    account_id: Some(ctx.account.id),
                                    message: format!("re-sync {}: {}", folder.path, e),
                                });
                            }
                        }
                        idle = backend
                            .idle(&inbox_path)
                            .await
                            .map_err(|e| SyncErrorReason::Other(format!("idle re-enter: {}", e)))?;
                    }
                    Err(e) => {
                        return Err(SyncErrorReason::Other(format!("idle: {}", e)));
                    }
                }
            }
        }
    }
}

async fn sync_folder_list<B: MailBackend>(
    ctx: &AccountTaskCtx,
    backend: &mut B,
) -> std::result::Result<Vec<Folder>, SyncErrorReason> {
    let infos = backend
        .list_folders()
        .await
        .map_err(|e| SyncErrorReason::Other(format!("list folders: {}", e)))?;

    let folder_repo = FolderRepo::new(ctx.db.pool());
    let existing = folder_repo
        .list_by_account(ctx.account.id)
        .await
        .map_err(|e| SyncErrorReason::Other(format!("list folders (db): {}", e)))?;
    let by_path: HashMap<String, &Folder> =
        existing.iter().map(|f| (f.path.clone(), f)).collect();

    let mut out = Vec::with_capacity(infos.len());
    for info in &infos {
        let prev = by_path.get(&info.path).copied();
        let id = prev.map(|f| f.id).unwrap_or_else(FolderId::new);
        let folder = to_folder(ctx.account.id, id, info, prev);
        folder_repo
            .upsert(&folder)
            .await
            .map_err(|e| SyncErrorReason::Other(format!("upsert folder: {}", e)))?;
        out.push(folder);
    }

    let _ = ctx.tx.send(SyncEvent::FolderListUpdated {
        account_id: ctx.account.id,
    });
    Ok(out)
}

async fn sync_one_folder<B: MailBackend>(
    ctx: &AccountTaskCtx,
    backend: &mut B,
    folder: &Folder,
) -> std::result::Result<(), SyncErrorReason> {
    let _ = ctx.tx.send(SyncEvent::SyncStarted {
        account_id: ctx.account.id,
        folder_id: Some(folder.id),
    });

    let folder_repo = FolderRepo::new(ctx.db.pool());
    let stored = folder_repo
        .get(folder.id)
        .await
        .ok();
    let last_uid_next = stored.as_ref().map(|f| f.uid_next).unwrap_or(0);

    let state = backend
        .select_folder(&folder.path)
        .await
        .map_err(|e| SyncErrorReason::Other(format!("select {}: {}", folder.path, e)))?;

    let needs_full_resync = stored
        .as_ref()
        .map(|s| s.uid_validity != 0 && s.uid_validity != state.uid_validity)
        .unwrap_or(false);

    let range = if needs_full_resync || last_uid_next == 0 {
        if state.uid_next > 1 {
            UidRange::Range(1, state.uid_next.saturating_sub(1).max(1))
        } else {
            UidRange::All
        }
    } else if state.uid_next > last_uid_next {
        UidRange::Range(last_uid_next, state.uid_next.saturating_sub(1).max(last_uid_next))
    } else {
        let _ = ctx.tx.send(SyncEvent::SyncFinished {
            account_id: ctx.account.id,
            folder_id: Some(folder.id),
        });
        return Ok(());
    };

    let range_dbg = format!("{:?}", range);
    info!(
        target: "imt-sync::account_task",
        folder = %folder.path,
        range = %range_dbg,
        last_uid_next = last_uid_next,
        server_uid_next = state.uid_next,
        server_exists = state.exists,
        "fetching envelopes"
    );
    let envelopes = backend
        .fetch_envelopes(&folder.path, range)
        .await
        .map_err(|e| SyncErrorReason::Other(format!("fetch envelopes: {}", e)))?;
    info!(
        target: "imt-sync::account_task",
        folder = %folder.path,
        count = envelopes.len(),
        "envelopes fetched"
    );

    let msg_repo = MessageRepo::new(ctx.db.pool());
    for env in envelopes {
        let existing = msg_repo
            .get_by_uid(folder.id, Uid(env.uid))
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
            None => to_message(ctx.account.id, folder.id, env),
        };
        let new_id = message.id;
        msg_repo
            .upsert_envelope(&message)
            .await
            .map_err(|e| SyncErrorReason::Other(format!("upsert message: {}", e)))?;
        let _ = ctx.tx.send(SyncEvent::MessageAdded {
            folder_id: folder.id,
            message_id: new_id,
        });
    }

    let updated = Folder {
        uid_validity: state.uid_validity,
        uid_next: state.uid_next,
        message_count: state.exists,
        unread_count: state.unseen,
        ..folder.clone()
    };
    folder_repo
        .upsert(&updated)
        .await
        .map_err(|e| SyncErrorReason::Other(format!("update folder counts: {}", e)))?;
    let _ = ctx.tx.send(SyncEvent::FolderCountsChanged {
        folder_id: folder.id,
        total: state.exists,
        unread: state.unseen,
    });
    let _ = ctx.tx.send(SyncEvent::SyncFinished {
        account_id: ctx.account.id,
        folder_id: Some(folder.id),
    });
    Ok(())
}

/// Helper trait to peek at `Notify` non-blockingly. Always returns false here
/// because `Notify` does not expose a non-blocking check; this is a placeholder
/// kept so the loop reads naturally and reserves a future enhancement point.
trait NotifyPeek {
    fn notified_now(&self) -> bool;
}

impl NotifyPeek for Notify {
    fn notified_now(&self) -> bool {
        false
    }
}

#[allow(dead_code)]
fn _force_use_unused_imports(
    _: &Address,
    _: &MessageHeaders,
    _: &Flag,
    _: &AccountId,
    _: &MessageId,
) {
    debug!("noop");
}
