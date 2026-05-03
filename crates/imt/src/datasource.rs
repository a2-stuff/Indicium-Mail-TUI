//! Adapter that exposes the live `SyncEngine` + `Snapshot` as the TUI's `DataSource`.

use std::sync::Arc;

use anyhow::Result;
use imt_core::{
    Account, AccountId, Draft, Flag, Folder, FolderId, Message, MessageBody, MessageId,
    NewAccountForm,
};
use imt_store::{secrets, Db, DraftRepo};
use imt_sync::SyncEngine;
use imt_tui::DataSource;
use tokio::sync::mpsc;

use crate::snapshot::Snapshot;

/// Asynchronous commands posted from the sync `DataSource` to a worker that
/// owns the `SyncEngine`. Fire-and-forget; results land via `SyncEvent`s.
pub enum Command {
    AddAccount { account: Account, password: String },
    SaveDraft(Draft),
    Send(Draft),
    FetchBody(MessageId),
    SetFlag { message_id: MessageId, flag: Flag, add: bool },
}

/// Sync-trait adapter the TUI talks to. All write methods enqueue a `Command`.
#[derive(Clone)]
pub struct SyncDataSource {
    pub snapshot: Snapshot,
    pub commands: mpsc::UnboundedSender<Command>,
}

impl SyncDataSource {
    pub fn new(snapshot: Snapshot, commands: mpsc::UnboundedSender<Command>) -> Self {
        Self { snapshot, commands }
    }
}

impl DataSource for SyncDataSource {
    fn accounts(&self) -> Vec<Account> {
        self.snapshot.read(|s| s.accounts.clone())
    }

    fn folders(&self, account: AccountId) -> Vec<Folder> {
        self.snapshot.read(|s| s.folders_by_account.get(&account).cloned().unwrap_or_default())
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
        let _ = self.commands.send(Command::FetchBody(message));
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

    fn toggle_flag(&self, message: MessageId) {
        self.snapshot.toggle_local_flag(message, Flag::Flagged);
        let was_flagged = self.snapshot.read(|s| {
            s.messages_by_folder.values().flatten().find(|m| m.id == message)
                .map(|m| m.flags.contains(&Flag::Flagged)).unwrap_or(false)
        });
        let _ = self.commands.send(Command::SetFlag { message_id: message, flag: Flag::Flagged, add: was_flagged });
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
        let account = form.into_account(order);
        let id = account.id;
        self.snapshot.add_local_account(account.clone());
        self.commands.send(Command::AddAccount { account, password })
            .map_err(|_| anyhow::anyhow!("engine channel closed"))?;
        Ok(id)
    }
}

/// Handle commands by dispatching to the `SyncEngine`. Runs until the
/// command channel is closed.
pub async fn command_worker(
    engine: Arc<SyncEngine>,
    db: Arc<Db>,
    snapshot: Snapshot,
    mut rx: mpsc::UnboundedReceiver<Command>,
) {
    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::AddAccount { account, password } => {
                let id = account.id;
                if let Err(e) = engine.add_account(account, password).await {
                    snapshot.set_status(format!("add account failed: {}", e));
                    snapshot.write(|s| s.accounts.retain(|a| a.id != id));
                }
            }
            Command::SaveDraft(draft) => {
                if let Err(e) = DraftRepo::new(db.pool()).upsert(&draft).await {
                    snapshot.set_status(format!("save draft failed: {}", e));
                }
            }
            Command::Send(draft) => {
                if let Err(e) = engine.send(&draft).await {
                    snapshot.set_status(format!("send failed: {}", e));
                } else {
                    snapshot.set_status("sent");
                }
            }
            Command::FetchBody(mid) => {
                match engine.fetch_body(mid).await {
                    Ok(body) => snapshot.write(|s| { s.bodies.insert(mid, body); }),
                    Err(e) => snapshot.set_status(format!("fetch body failed: {}", e)),
                }
            }
            Command::SetFlag { message_id, flag, add } => {
                let acc = snapshot.read(|s| {
                    s.messages_by_folder.values().flatten()
                        .find(|m| m.id == message_id)
                        .map(|m| (m.account_id, m.folder_id, m.uid))
                });
                if let Some((_acc_id, folder_id, uid)) = acc {
                    let folder = snapshot.read(|s| {
                        s.folders_by_account.values().flatten()
                            .find(|f| f.id == folder_id).cloned()
                    });
                    if let Some(folder) = folder {
                        let acc = snapshot.read(|s| {
                            s.accounts.iter().find(|a| a.id == folder.account_id).cloned()
                        });
                        if let Some(acc) = acc {
                            let provider = imt_sync::password::imap_provider_for(acc.id);
                            let _ = secrets::load(acc.id, "imap_password");
                            let mut backend = imt_net::ImapBackend::new(acc, provider);
                            use imt_net::backend::MailBackend;
                            if backend.connect().await.is_ok() {
                                let (add_v, rem_v): (Vec<Flag>, Vec<Flag>) = if add {
                                    (vec![flag.clone()], Vec::new())
                                } else {
                                    (Vec::new(), vec![flag.clone()])
                                };
                                let _ = backend.set_flags(&folder.path, uid.0, &add_v, &rem_v).await;
                                let _ = backend.disconnect().await;
                            }
                        }
                    }
                }
            }
        }
    }
}

