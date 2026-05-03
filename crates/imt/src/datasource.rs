//! Adapter that exposes the live `SyncEngine` + `Snapshot` as the TUI's `DataSource`.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use anyhow::Result;
use imt_core::{
    Account, AccountId, Draft, Flag, Folder, FolderId, FolderRole, Message, MessageBody, MessageId,
    NewAccountForm,
};

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
use imt_store::{Db, DraftRepo};
use imt_sync::SyncEngine;
use imt_tui::DataSource;
use tokio::sync::mpsc;

use crate::snapshot::Snapshot;

/// Asynchronous commands posted from the sync `DataSource` to a worker that
/// owns the `SyncEngine`. Fire-and-forget; results land via `SyncEvent`s.
pub enum Command {
    AddAccount { account: Account, password: String, oauth_exchange: Option<imt_sync::engine::OAuthExchange> },
    UpdateAccount { account: Account, password: Option<String> },
    DeleteAccount { id: AccountId },
    SaveDraft(Draft),
    Send(Draft),
    FetchBody(MessageId),
    SetFlag { message_id: MessageId, flag: Flag, add: bool },
    SyncFolder { account: AccountId, folder: FolderId },
    SyncAccount { account: AccountId },
    SyncAll,
    Move { message_id: MessageId, dest_folder: FolderId },
    Delete { message_id: MessageId },
}

/// Sync-trait adapter the TUI talks to. All write methods enqueue a `Command`.
#[derive(Clone)]
pub struct SyncDataSource {
    pub snapshot: Snapshot,
    pub commands: mpsc::UnboundedSender<Command>,
    pub in_flight_bodies: Arc<Mutex<HashSet<MessageId>>>,
}

impl SyncDataSource {
    pub fn new(snapshot: Snapshot, commands: mpsc::UnboundedSender<Command>) -> Self {
        Self {
            snapshot,
            commands,
            in_flight_bodies: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl DataSource for SyncDataSource {
    fn accounts(&self) -> Vec<Account> {
        self.snapshot.read(|s| s.accounts.clone())
    }

    fn folders(&self, account: AccountId) -> Vec<Folder> {
        let mut folders = self
            .snapshot
            .read(|s| s.folders_by_account.get(&account).cloned().unwrap_or_default());
        folders.sort_by_key(|f| (folder_sort_key(f.role), f.name.to_lowercase()));
        folders
    }

    fn messages(&self, folder: FolderId) -> Vec<Message> {
        self.snapshot.read(|s| {
            let mut msgs = s.messages_by_folder.get(&folder).cloned().unwrap_or_default();
            msgs.sort_by(|a, b| b.internal_date.cmp(&a.internal_date));
            msgs
        })
    }

    fn message_body(&self, message: MessageId) -> Option<MessageBody> {
        let cached = self.snapshot.read(|s| s.bodies.get(&message).cloned());
        if cached.is_some() {
            return cached;
        }
        let mut inflight = self.in_flight_bodies.lock().unwrap();
        if inflight.insert(message) {
            let _ = self.commands.send(Command::FetchBody(message));
        }
        None
    }

    fn save_draft(&self, draft: &Draft) -> Result<()> {
        self.commands.send(Command::SaveDraft(draft.clone()))
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(())
    }

    fn send(&self, draft: &Draft) -> Result<()> {
        self.commands.send(Command::Send(draft.clone()))
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(())
    }

    fn mark_read(&self, message: MessageId) {
        self.snapshot.mark_local_read(message);
        let _ = self.commands.send(Command::SetFlag { message_id: message, flag: Flag::Seen, add: true });
    }

    fn set_seen(&self, message: MessageId, seen: bool) {
        self.snapshot.write(|s| {
            for msgs in s.messages_by_folder.values_mut() {
                if let Some(m) = msgs.iter_mut().find(|m| m.id == message) {
                    let has = m.flags.contains(&Flag::Seen);
                    if seen && !has {
                        m.flags.push(Flag::Seen);
                    } else if !seen && has {
                        m.flags.retain(|f| f != &Flag::Seen);
                    }
                }
            }
        });
        let _ = self.commands.send(Command::SetFlag { message_id: message, flag: Flag::Seen, add: seen });
    }

    fn toggle_flag(&self, message: MessageId) {
        self.snapshot.toggle_local_flag(message, Flag::Flagged);
        let was_flagged = self.snapshot.read(|s| {
            s.messages_by_folder.values().flatten().find(|m| m.id == message)
                .map(|m| m.flags.contains(&Flag::Flagged)).unwrap_or(false)
        });
        let _ = self.commands.send(Command::SetFlag { message_id: message, flag: Flag::Flagged, add: was_flagged });
    }

    fn move_message(&self, message: MessageId, dest_folder: FolderId) -> anyhow::Result<()> {
        let src_folder = self.snapshot.read(|s| {
            s.messages_by_folder.iter()
                .find(|(_, msgs)| msgs.iter().any(|m| m.id == message))
                .map(|(fid, _)| *fid)
        });
        if let Some(src) = src_folder {
            self.snapshot.write(|s| {
                if let Some(msgs) = s.messages_by_folder.get_mut(&src) {
                    if let Some(pos) = msgs.iter().position(|m| m.id == message) {
                        let mut m = msgs.remove(pos);
                        m.folder_id = dest_folder;
                        s.messages_by_folder.entry(dest_folder).or_default().push(m);
                    }
                }
            });
        }
        self.commands
            .send(Command::Move { message_id: message, dest_folder })
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(())
    }

    fn delete_message(&self, message: MessageId) -> anyhow::Result<()> {
        let trash_id = self.snapshot.read(|s| {
            s.messages_by_folder.iter()
                .find(|(_, msgs)| msgs.iter().any(|m| m.id == message))
                .and_then(|(_fid, msgs)| msgs.iter().find(|m| m.id == message).map(|m| m.account_id))
                .and_then(|aid| s.folders_by_account.get(&aid))
                .and_then(|fs| fs.iter().find(|f| f.role == imt_core::FolderRole::Trash).map(|f| f.id))
        });
        if let Some(dest) = trash_id {
            return self.move_message(message, dest);
        }
        self.snapshot.write(|s| {
            for msgs in s.messages_by_folder.values_mut() {
                msgs.retain(|m| m.id != message);
            }
        });
        self.commands
            .send(Command::Delete { message_id: message })
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(())
    }

    fn search(&self, query: &str) -> Vec<MessageId> {
        let q = query.to_lowercase();
        if q.is_empty() {
            return Vec::new();
        }
        self.snapshot.read(|s| {
            let mut out = Vec::new();
            for msgs in s.messages_by_folder.values() {
                for m in msgs {
                    let hay = format!(
                        "{} {} {}",
                        m.headers.subject,
                        m.headers.from.iter().map(|a| a.format()).collect::<Vec<_>>().join(" "),
                        m.snippet
                    );
                    if hay.to_lowercase().contains(&q) {
                        out.push(m.id);
                    }
                }
            }
            out
        })
    }

    fn add_account(&self, form: NewAccountForm) -> Result<AccountId> {
        let order = self.snapshot.read(|s| s.accounts.len() as i32);
        let password = form.password.clone();
        let oauth_exchange = if form.is_oauth2() && !form.oauth_code.is_empty() {
            Some(imt_sync::engine::OAuthExchange {
                client_id: form.oauth_client_id.clone(),
                client_secret: form.oauth_client_secret.clone(),
                code: form.oauth_code.clone(),
                verifier: form.oauth_verifier.clone(),
                redirect_uri: form.oauth_redirect_uri.clone(),
            })
        } else {
            None
        };
        let account = form.into_account(order);
        let id = account.id;
        self.snapshot.add_local_account(account.clone());
        self.commands.send(Command::AddAccount { account, password, oauth_exchange })
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(id)
    }

    fn update_account(&self, id: AccountId, form: NewAccountForm, pw_changed: bool) -> Result<()> {
        let existing_order = self.snapshot.read(|s| {
            s.accounts.iter().find(|a| a.id == id).map(|a| a.order).unwrap_or(0)
        });
        let password = if pw_changed { Some(form.password.clone()) } else { None };
        let mut account = form.into_account(existing_order);
        account.id = id;
        self.snapshot.write(|s| {
            if let Some(pos) = s.accounts.iter().position(|a| a.id == id) {
                s.accounts[pos] = account.clone();
            }
        });
        self.commands
            .send(Command::UpdateAccount { account, password })
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(())
    }

    fn delete_account(&self, id: AccountId) -> Result<()> {
        self.snapshot.write(|s| {
            s.accounts.retain(|a| a.id != id);
            s.folders_by_account.remove(&id);
        });
        self.commands
            .send(Command::DeleteAccount { id })
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(())
    }

    fn status(&self) -> String {
        self.snapshot.read(|s| s.status.clone())
    }

    fn pop_notification(&self) -> Option<String> {
        self.snapshot.pop_notification()
    }

    fn refresh(&self, account: Option<AccountId>, folder: Option<FolderId>) {
        let cmd = match (account, folder) {
            (Some(a), Some(f)) => Command::SyncFolder { account: a, folder: f },
            (Some(a), None) => Command::SyncAccount { account: a },
            _ => Command::SyncAll,
        };
        let _ = self.commands.send(cmd);
    }
}

/// Handle commands by dispatching to the `SyncEngine`. Runs until the
/// command channel is closed.
pub async fn command_worker(
    engine: Arc<SyncEngine>,
    db: Arc<Db>,
    snapshot: Snapshot,
    in_flight_bodies: Arc<Mutex<HashSet<MessageId>>>,
    mut rx: mpsc::UnboundedReceiver<Command>,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::AddAccount { account, password, oauth_exchange } => {
                let id = account.id;
                if let Err(e) = engine.add_account(account, password, oauth_exchange).await {
                    snapshot.push_notification(format!("Add account failed: {}", e));
                    snapshot.write(|s| s.accounts.retain(|a| a.id != id));
                }
            }
            Command::UpdateAccount { account, password } => {
                let id = account.id;
                if let Err(e) = engine.update_account(account, password).await {
                    snapshot.push_notification(format!("Update account failed: {}", e));
                } else {
                    snapshot.push_notification("Account updated".to_string());
                    let _ = id;
                }
            }
            Command::DeleteAccount { id } => {
                if let Err(e) = engine.remove_account(id).await {
                    snapshot.push_notification(format!("Delete account failed: {}", e));
                } else {
                    snapshot.push_notification("Account deleted".to_string());
                }
            }
            Command::SaveDraft(draft) => {
                if let Err(e) = DraftRepo::new(db.pool()).upsert(&draft).await {
                    snapshot.push_notification(format!("Save draft failed: {}", e));
                } else {
                    // Best-effort: also append to IMAP Drafts folder so it appears when navigating there.
                    if let Err(e) = engine.save_draft_to_imap(&draft).await {
                        tracing::warn!("save_draft_to_imap: {}", e);
                    } else {
                        // Re-sync the Drafts folder so the new draft appears immediately.
                        let drafts_folder = snapshot.read(|s| {
                            s.folders_by_account.values()
                                .flat_map(|fs| fs.iter())
                                .find(|f| f.role == imt_core::FolderRole::Drafts)
                                .map(|f| (f.account_id, f.id))
                        });
                        if let Some((aid, fid)) = drafts_folder {
                            if let Err(e) = engine.sync_folder(aid, fid).await {
                                tracing::warn!("sync drafts folder: {}", e);
                            }
                        }
                    }
                    snapshot.push_notification("Draft saved".to_string());
                }
            }
            Command::Send(draft) => {
                if let Err(e) = engine.send(&draft).await {
                    snapshot.push_notification(format!("Send failed: {}", e));
                } else {
                    snapshot.push_notification("Sent".to_string());
                }
            }
            Command::FetchBody(mid) => {
                match engine.fetch_body(mid).await {
                    Ok(body) => snapshot.write(|s| { s.bodies.insert(mid, body); }),
                    Err(e) => snapshot.push_notification(format!("Fetch body failed: {}", e)),
                }
                in_flight_bodies.lock().unwrap().remove(&mid);
            }
            Command::SyncFolder { account, folder } => {
                if let Err(e) = engine.sync_folder(account, folder).await {
                    snapshot.push_notification(format!("Sync failed: {}", e));
                }
            }
            Command::SyncAccount { account } => {
                let folder_ids = snapshot.read(|s| {
                    s.folders_by_account.get(&account)
                        .map(|fs| fs.iter().map(|f| f.id).collect::<Vec<_>>())
                        .unwrap_or_default()
                });
                for fid in folder_ids {
                    if let Err(e) = engine.sync_folder(account, fid).await {
                        snapshot.push_notification(format!("Sync failed: {}", e));
                    }
                }
            }
            Command::Move { message_id, dest_folder } => {
                if let Err(e) = engine.move_message(message_id, dest_folder).await {
                    snapshot.push_notification(format!("Move failed: {}", e));
                }
            }
            Command::Delete { message_id } => {
                let _ = message_id;
                snapshot.push_notification("No Trash folder found - message kept on server".to_string());
            }
            Command::SyncAll => {
                let pairs = snapshot.read(|s| {
                    s.folders_by_account.iter()
                        .flat_map(|(aid, fs)| fs.iter().map(move |f| (*aid, f.id)))
                        .collect::<Vec<_>>()
                });
                for (aid, fid) in pairs {
                    if let Err(e) = engine.sync_folder(aid, fid).await {
                        snapshot.push_notification(format!("Sync failed: {}", e));
                    }
                }
            }
            Command::SetFlag { message_id, flag, add } => {
                if let Err(e) = engine.set_flag(message_id, flag, add).await {
                    snapshot.push_notification(format!("Set flag failed: {}", e));
                }
            }
        }
    }
}

