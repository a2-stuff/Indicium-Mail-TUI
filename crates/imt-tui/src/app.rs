//! Application state machine.

use std::collections::HashSet;
use std::sync::Arc;

use crossterm::event::KeyEvent;
use imt_core::{Account, AccountId, Address, Draft, Folder, FolderId, Message, MessageBody, NewAccountForm, Tls};
use tui_input::backend::crossterm::EventHandler;
use tui_input::Input;
use tui_textarea::TextArea;

use crate::data::{empty_draft, DataSource};
use crate::keymap::{map_key, ComposeField, Focus, KeyAction, Mode, OnboardingField};
use crate::presets::preset_for;
use crate::quote::{build_forward, build_reply};

/// Per-account view state in the sidebar.
#[derive(Debug, Clone)]
pub struct AccountView {
    pub account: Account,
    pub folders: Vec<Folder>,
    pub expanded: bool,
}

/// State of the compose modal.
pub struct ComposeState {
    pub draft: Draft,
    pub field: ComposeField,
    pub to: Input,
    pub cc: Input,
    pub bcc: Input,
    pub subject: Input,
    pub body: TextArea<'static>,
    pub from_idx: usize,
}

impl ComposeState {
    fn from_draft(draft: Draft, accounts: &[Account]) -> Self {
        let to = Input::new(addr_join(&draft.to));
        let cc = Input::new(addr_join(&draft.cc));
        let bcc = Input::new(addr_join(&draft.bcc));
        let subject = Input::new(draft.subject.clone());
        let mut body = TextArea::new(draft.body_text.lines().map(String::from).collect());
        body.set_block(
            ratatui::widgets::Block::default()
                .borders(ratatui::widgets::Borders::ALL)
                .border_type(ratatui::widgets::BorderType::Rounded)
                .title("Body"),
        );
        let from_idx = accounts.iter().position(|a| a.id == draft.account_id).unwrap_or(0);
        Self { draft, field: ComposeField::To, to, cc, bcc, subject, body, from_idx }
    }

    /// Read inputs back into the underlying draft.
    pub fn sync_to_draft(&mut self, accounts: &[Account]) {
        self.draft.to = parse_addrs(self.to.value());
        self.draft.cc = parse_addrs(self.cc.value());
        self.draft.bcc = parse_addrs(self.bcc.value());
        self.draft.subject = self.subject.value().to_string();
        self.draft.body_text = self.body.lines().join("\n");
        if let Some(acc) = accounts.get(self.from_idx) {
            self.draft.account_id = acc.id;
            self.draft.from = acc.address.clone();
        }
        self.draft.updated_at = chrono::Utc::now();
    }
}

fn addr_join(v: &[Address]) -> String {
    v.iter().map(|a| a.format()).collect::<Vec<_>>().join(", ")
}

fn accept_numeric(key: crossterm::event::KeyEvent) -> bool {
    use crossterm::event::KeyCode;
    match key.code {
        KeyCode::Char(c) => c.is_ascii_digit(),
        KeyCode::Backspace | KeyCode::Delete | KeyCode::Left | KeyCode::Right
        | KeyCode::Home | KeyCode::End => true,
        _ => false,
    }
}

fn parse_addrs(s: &str) -> Vec<Address> {
    s.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| {
            if let (Some(lt), Some(gt)) = (s.find('<'), s.rfind('>')) {
                let name = s[..lt].trim().trim_matches('"');
                let email = s[lt + 1..gt].trim();
                if name.is_empty() {
                    Address::new(email)
                } else {
                    Address::named(name, email)
                }
            } else {
                Address::new(s)
            }
        })
        .collect()
}

/// State of the account onboarding modal.
pub struct OnboardingState {
    /// Currently focused field.
    pub field: OnboardingField,
    /// Display name input.
    pub display_name: Input,
    /// Email input.
    pub email: Input,
    /// IMAP host input.
    pub imap_host: Input,
    /// IMAP port input (numeric).
    pub imap_port: Input,
    /// IMAP TLS cycler.
    pub imap_tls: Tls,
    /// SMTP host input.
    pub smtp_host: Input,
    /// SMTP port input (numeric).
    pub smtp_port: Input,
    /// SMTP TLS cycler.
    pub smtp_tls: Tls,
    /// Username input (defaults to email when blank).
    pub username: Input,
    /// Password input (rendered masked).
    pub password: Input,
    /// Tracks which host/port/tls fields have been manually edited so a preset
    /// won't overwrite user input.
    pub user_edited_imap: bool,
    pub user_edited_smtp: bool,
    /// Detected provider name (when a preset was applied).
    pub detected_provider: Option<String>,
    /// Last-applied email domain for preset detection.
    pub last_preset_domain: Option<String>,
}

impl OnboardingState {
    /// Build a fresh onboarding state from form defaults.
    pub fn new() -> Self {
        let defaults = NewAccountForm::defaults();
        Self {
            field: OnboardingField::DisplayName,
            display_name: Input::default(),
            email: Input::default(),
            imap_host: Input::new(defaults.imap_host),
            imap_port: Input::new(defaults.imap_port.to_string()),
            imap_tls: defaults.imap_tls,
            smtp_host: Input::new(defaults.smtp_host),
            smtp_port: Input::new(defaults.smtp_port.to_string()),
            smtp_tls: defaults.smtp_tls,
            username: Input::default(),
            password: Input::default(),
            user_edited_imap: false,
            user_edited_smtp: false,
            detected_provider: None,
            last_preset_domain: None,
        }
    }

    /// Build a `NewAccountForm` from the current state, validating numeric fields.
    pub fn to_form(&self) -> anyhow::Result<NewAccountForm> {
        let email = self.email.value().trim().to_string();
        if email.is_empty() {
            anyhow::bail!("Email is required");
        }
        let imap_host = self.imap_host.value().trim().to_string();
        if imap_host.is_empty() {
            anyhow::bail!("IMAP host is required");
        }
        let smtp_host = self.smtp_host.value().trim().to_string();
        if smtp_host.is_empty() {
            anyhow::bail!("SMTP host is required");
        }
        let imap_port: u16 = self
            .imap_port
            .value()
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("IMAP port must be a number"))?;
        let smtp_port: u16 = self
            .smtp_port
            .value()
            .trim()
            .parse()
            .map_err(|_| anyhow::anyhow!("SMTP port must be a number"))?;
        let username = if self.username.value().trim().is_empty() {
            email.clone()
        } else {
            self.username.value().trim().to_string()
        };
        Ok(NewAccountForm {
            display_name: self.display_name.value().trim().to_string(),
            email,
            imap_host,
            imap_port,
            imap_tls: self.imap_tls,
            smtp_host,
            smtp_port,
            smtp_tls: self.smtp_tls,
            username,
            password: self.password.value().to_string(),
        })
    }
}

impl Default for OnboardingState {
    fn default() -> Self {
        Self::new()
    }
}

/// Main application state.
pub struct App {
    /// Backing data source.
    pub data: Arc<dyn DataSource>,
    /// Currently focused pane.
    pub focus: Focus,
    /// Top-level mode.
    pub mode: Mode,
    /// Set when the event loop should exit.
    pub should_quit: bool,
    /// Account/folder rows in the sidebar.
    pub accounts: Vec<AccountView>,
    pub sidebar_account_idx: usize,
    pub sidebar_folder_idx: usize,
    pub messages: Vec<Message>,
    pub message_idx: usize,
    pub current_body: Option<MessageBody>,
    pub reader_scroll: u16,
    pub status: String,
    pub search_input: Input,
    pub search_results: HashSet<imt_core::MessageId>,
    pub compose: Option<ComposeState>,
    pub last_error: Option<String>,
    /// Onboarding modal state.
    pub onboarding: Option<OnboardingState>,
    /// When true, HTML-only bodies are not rendered inline; user is invited to open in browser.
    pub html_external: bool,
    /// Browser command to spawn for HTML viewing. Empty falls back to `xdg-open`.
    pub browser: String,
}

impl App {
    /// Build a new `App` from a data source.
    pub fn new(data: Arc<dyn DataSource>) -> Self {
        let accounts_raw = data.accounts();
        let accounts: Vec<AccountView> = accounts_raw
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                let folders = data.folders(a.id);
                AccountView { account: a, folders, expanded: i == 0 }
            })
            .collect();

        let initial_folder_idx = accounts
            .first()
            .and_then(|a| a.folders.iter().position(|f| f.role == imt_core::FolderRole::Inbox))
            .unwrap_or(0);

        let mut app = Self {
            data,
            focus: Focus::MessageList,
            mode: Mode::Normal,
            should_quit: false,
            accounts,
            sidebar_account_idx: 0,
            sidebar_folder_idx: initial_folder_idx,
            messages: Vec::new(),
            message_idx: 0,
            current_body: None,
            reader_scroll: 0,
            status: String::new(),
            search_input: Input::default(),
            search_results: HashSet::new(),
            compose: None,
            last_error: None,
            onboarding: None,
            html_external: false,
            browser: String::new(),
        };
        app.refresh_messages();
        if app.accounts.is_empty() {
            app.open_onboarding();
        }
        app
    }

    /// Toggle external HTML viewer mode and configure the browser command.
    /// An empty `browser` falls back to `xdg-open`.
    pub fn set_html_external(&mut self, on: bool, browser: String) {
        self.html_external = on;
        self.browser = browser;
    }

    /// Open the onboarding modal with a fresh form.
    pub fn open_onboarding(&mut self) {
        self.onboarding = Some(OnboardingState::new());
        self.mode = Mode::Onboarding;
        self.last_error = None;
    }

    /// The currently selected account, if any.
    pub fn current_account(&self) -> Option<&Account> {
        self.accounts.get(self.sidebar_account_idx).map(|a| &a.account)
    }

    /// The currently selected folder, if any.
    pub fn current_folder(&self) -> Option<&Folder> {
        self.accounts
            .get(self.sidebar_account_idx)
            .and_then(|a| a.folders.get(self.sidebar_folder_idx))
    }

    /// The currently selected message, if any.
    pub fn current_message(&self) -> Option<&Message> {
        self.messages.get(self.message_idx)
    }

    /// Refresh the message list from the selected folder.
    pub fn refresh_messages(&mut self) {
        if let Some(f) = self.current_folder() {
            let id = f.id;
            self.messages = self.data.messages(id);
        } else {
            self.messages.clear();
        }
        if self.message_idx >= self.messages.len() {
            self.message_idx = self.messages.len().saturating_sub(1);
        }
        self.refresh_body();
    }

    fn refresh_body(&mut self) {
        self.current_body = self
            .current_message()
            .and_then(|m| self.data.message_body(m.id).or_else(|| m.body.clone()));
        self.reader_scroll = 0;
    }

    /// Periodic tick (every 250ms). Pulls fresh state from the data source so
    /// background sync events become visible without manual interaction.
    pub fn tick(&mut self) {
        let new_accounts = self.data.accounts();
        let accounts_changed = new_accounts.len() != self.accounts.len()
            || new_accounts
                .iter()
                .zip(self.accounts.iter())
                .any(|(a, av)| a.id != av.account.id);
        if accounts_changed {
            let prev_expanded: std::collections::HashMap<imt_core::AccountId, bool> = self
                .accounts
                .iter()
                .map(|av| (av.account.id, av.expanded))
                .collect();
            self.accounts = new_accounts
                .into_iter()
                .enumerate()
                .map(|(i, a)| {
                    let folders = self.data.folders(a.id);
                    let expanded = prev_expanded.get(&a.id).copied().unwrap_or(i == 0);
                    AccountView { account: a, folders, expanded }
                })
                .collect();
        } else {
            for av in self.accounts.iter_mut() {
                av.folders = self.data.folders(av.account.id);
            }
        }

        if self.sidebar_account_idx >= self.accounts.len() {
            self.sidebar_account_idx = self.accounts.len().saturating_sub(1);
        }
        if let Some(a) = self.accounts.get(self.sidebar_account_idx) {
            if self.sidebar_folder_idx >= a.folders.len() {
                self.sidebar_folder_idx = a.folders.len().saturating_sub(1);
            }
        }

        if let Some(folder_id) = self.current_folder().map(|f| f.id) {
            let new_msgs = self.data.messages(folder_id);
            let len_changed = new_msgs.len() != self.messages.len();
            let head_changed = !len_changed
                && new_msgs.first().map(|m| m.id) != self.messages.first().map(|m| m.id);
            let flags_changed = !len_changed
                && !head_changed
                && new_msgs
                    .iter()
                    .zip(self.messages.iter())
                    .any(|(a, b)| a.flags != b.flags);
            if len_changed || head_changed || flags_changed {
                let selected_id = self.current_message().map(|m| m.id);
                self.messages = new_msgs;
                self.message_idx = selected_id
                    .and_then(|id| self.messages.iter().position(|m| m.id == id))
                    .unwrap_or(0);
            }
        }

        if self.current_body.is_none() {
            if let Some(m) = self.current_message() {
                if let Some(body) = self.data.message_body(m.id).or_else(|| m.body.clone()) {
                    self.current_body = Some(body);
                }
            }
        }
    }

    /// Handle a raw key event.
    pub fn handle_key(&mut self, key: KeyEvent) {
        if self.mode == Mode::Compose {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
                return;
            }
            if let Some(c) = self.compose.as_mut() {
                match c.field {
                    ComposeField::To => { c.to.handle_event(&crossterm::event::Event::Key(key)); }
                    ComposeField::Cc => { c.cc.handle_event(&crossterm::event::Event::Key(key)); }
                    ComposeField::Bcc => { c.bcc.handle_event(&crossterm::event::Event::Key(key)); }
                    ComposeField::Subject => { c.subject.handle_event(&crossterm::event::Event::Key(key)); }
                    ComposeField::Body => { c.body.input(key); }
                    ComposeField::From | ComposeField::Attachments => {
                        if matches!(key.code, crossterm::event::KeyCode::Left) && c.field == ComposeField::From {
                            if c.from_idx > 0 { c.from_idx -= 1; }
                        }
                        if matches!(key.code, crossterm::event::KeyCode::Right) && c.field == ComposeField::From {
                            if c.from_idx + 1 < self.accounts.len() { c.from_idx += 1; }
                        }
                    }
                }
            }
            return;
        }
        if self.mode == Mode::Search {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
                return;
            }
            self.search_input.handle_event(&crossterm::event::Event::Key(key));
            self.run_search();
            return;
        }
        if self.mode == Mode::Onboarding {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
                return;
            }
            if let Some(o) = self.onboarding.as_mut() {
                let evt = crossterm::event::Event::Key(key);
                let mut email_changed = false;
                match o.field {
                    OnboardingField::DisplayName => { o.display_name.handle_event(&evt); }
                    OnboardingField::Email => {
                        o.email.handle_event(&evt);
                        email_changed = true;
                    }
                    OnboardingField::ImapHost => {
                        o.imap_host.handle_event(&evt);
                        o.user_edited_imap = true;
                    }
                    OnboardingField::ImapPort => {
                        if accept_numeric(key) {
                            o.imap_port.handle_event(&evt);
                            o.user_edited_imap = true;
                        }
                    }
                    OnboardingField::SmtpHost => {
                        o.smtp_host.handle_event(&evt);
                        o.user_edited_smtp = true;
                    }
                    OnboardingField::SmtpPort => {
                        if accept_numeric(key) {
                            o.smtp_port.handle_event(&evt);
                            o.user_edited_smtp = true;
                        }
                    }
                    OnboardingField::Username => { o.username.handle_event(&evt); }
                    OnboardingField::Password => { o.password.handle_event(&evt); }
                    OnboardingField::ImapTls | OnboardingField::SmtpTls => {}
                }
                if email_changed {
                    self.maybe_apply_preset();
                }
            }
            return;
        }
        if let Some(action) = map_key(self.focus, self.mode, key) {
            self.dispatch(action);
        }
    }

    fn maybe_apply_preset(&mut self) {
        let o = match self.onboarding.as_mut() {
            Some(o) => o,
            None => return,
        };
        let email = o.email.value().to_string();
        let domain = email.split('@').nth(1).map(|s| s.to_ascii_lowercase());
        if domain == o.last_preset_domain {
            return;
        }
        if let Some(d) = domain.as_ref() {
            if let Some(preset) = preset_for(&email) {
                if !o.user_edited_imap {
                    o.imap_host = Input::new(preset.imap_host);
                    o.imap_port = Input::new(preset.imap_port.to_string());
                    o.imap_tls = preset.imap_tls;
                }
                if !o.user_edited_smtp {
                    o.smtp_host = Input::new(preset.smtp_host);
                    o.smtp_port = Input::new(preset.smtp_port.to_string());
                    o.smtp_tls = preset.smtp_tls;
                }
                o.detected_provider = Some(d.clone());
                o.last_preset_domain = Some(d.clone());
                return;
            }
        }
        o.detected_provider = None;
        o.last_preset_domain = domain;
    }

    fn run_search(&mut self) {
        let q = self.search_input.value().to_string();
        self.search_results = self.data.search(&q).into_iter().collect();
        self.status = format!("{} match(es)", self.search_results.len());
    }

    /// Dispatch a high-level action.
    pub fn dispatch(&mut self, action: KeyAction) {
        match action {
            KeyAction::Quit => self.should_quit = true,
            KeyAction::Help => {
                self.mode = if self.mode == Mode::Help { Mode::Normal } else { Mode::Help };
            }
            KeyAction::FocusNext => self.focus_next(),
            KeyAction::FocusPrev => self.focus_prev(),
            KeyAction::Up => self.move_up(),
            KeyAction::Down => self.move_down(),
            KeyAction::PageUp => self.page(-10),
            KeyAction::PageDown => self.page(10),
            KeyAction::Top => self.goto(0),
            KeyAction::Bottom => self.goto(usize::MAX),
            KeyAction::OpenMessage => self.open_message(),
            KeyAction::BackToList => self.back_to_list(),
            KeyAction::Compose => self.start_compose_new(),
            KeyAction::Reply => self.start_reply(false),
            KeyAction::ReplyAll => self.start_reply(true),
            KeyAction::Forward => self.start_forward(),
            KeyAction::Delete => self.delete_current(),
            KeyAction::ToggleFlag => self.toggle_flag(),
            KeyAction::MarkRead => self.mark_read(),
            KeyAction::Search => {
                self.mode = Mode::Search;
                self.search_input = Input::default();
                self.search_results.clear();
                self.status = "Search".into();
            }
            KeyAction::GotoFolder => {}
            KeyAction::NextAccount => self.cycle_account(1),
            KeyAction::PrevAccount => self.cycle_account(-1),
            KeyAction::Send => self.send_compose(),
            KeyAction::SaveDraft => self.save_compose_draft(),
            KeyAction::CancelCompose => {
                self.compose = None;
                self.mode = Mode::Normal;
                self.status = "Cancelled".into();
            }
            KeyAction::AddAttachment => {
                self.status = "Attachments not implemented in mock".into();
            }
            KeyAction::OpenOnboarding => self.open_onboarding(),
            KeyAction::SaveOnboarding => self.save_onboarding(),
            KeyAction::CancelOnboarding => {
                self.onboarding = None;
                self.mode = Mode::Normal;
                self.status = "Cancelled".into();
            }
            KeyAction::OnboardingCycleLeft => self.cycle_tls(-1),
            KeyAction::OnboardingCycleRight => self.cycle_tls(1),
            KeyAction::OpenHtmlInBrowser => self.open_html_in_browser(),
            KeyAction::Refresh => {
                let acc = self.current_account().map(|a| a.id);
                let folder = self.current_folder().map(|f| f.id);
                self.data.refresh(acc, folder);
                self.status = "refreshing...".into();
            }
        }
    }

    fn cycle_tls(&mut self, delta: i32) {
        let o = match self.onboarding.as_mut() {
            Some(o) => o,
            None => return,
        };
        let target = match o.field {
            OnboardingField::ImapTls => &mut o.imap_tls,
            OnboardingField::SmtpTls => &mut o.smtp_tls,
            _ => return,
        };
        let order = [Tls::Implicit, Tls::StartTls, Tls::None];
        let cur = order.iter().position(|t| *t == *target).unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(order.len() as i32) as usize;
        *target = order[next];
        // Mark the relevant section as user-edited so presets won't overwrite.
        match o.field {
            OnboardingField::ImapTls => o.user_edited_imap = true,
            OnboardingField::SmtpTls => o.user_edited_smtp = true,
            _ => {}
        }
    }

    fn save_onboarding(&mut self) {
        let form = match self.onboarding.as_ref().map(|o| o.to_form()) {
            Some(Ok(f)) => f,
            Some(Err(e)) => {
                self.status = format!("Invalid: {e}");
                self.last_error = Some(e.to_string());
                return;
            }
            None => return,
        };
        match self.data.add_account(form) {
            Ok(id) => {
                let accounts_raw = self.data.accounts();
                self.accounts = accounts_raw
                    .into_iter()
                    .enumerate()
                    .map(|(i, a)| {
                        let folders = self.data.folders(a.id);
                        AccountView { account: a, folders, expanded: i == 0 }
                    })
                    .collect();
                if let Some(idx) = self.accounts.iter().position(|a| a.account.id == id) {
                    self.sidebar_account_idx = idx;
                    self.sidebar_folder_idx = 0;
                    if let Some(av) = self.accounts.get_mut(idx) {
                        av.expanded = true;
                    }
                }
                self.refresh_messages();
                self.onboarding = None;
                self.mode = Mode::Normal;
                self.status = "Account added".into();
            }
            Err(e) => {
                self.status = format!("Add failed: {e}");
                self.last_error = Some(e.to_string());
            }
        }
    }

    fn open_html_in_browser(&mut self) {
        let html = match self.current_body.as_ref().and_then(|b| b.text_html.clone()) {
            Some(h) => h,
            None => {
                self.status = "No HTML body".into();
                return;
            }
        };
        let path = std::env::temp_dir().join(format!("imt-{}.html", uuid::Uuid::new_v4()));
        if let Err(e) = std::fs::write(&path, html.as_bytes()) {
            self.status = format!("Failed to open: {e}");
            return;
        }
        let cmd = if self.browser.is_empty() { "xdg-open".to_string() } else { self.browser.clone() };
        let path_buf = path.clone();
        tokio::spawn(async move {
            let _ = tokio::process::Command::new(&cmd).arg(path_buf).spawn();
        });
        self.status = "Opened in browser".into();
    }

    fn focus_next(&mut self) {
        if self.mode == Mode::Compose {
            if let Some(c) = self.compose.as_mut() {
                c.field = c.field.next();
            }
            return;
        }
        if self.mode == Mode::Onboarding {
            if let Some(o) = self.onboarding.as_mut() {
                o.field = o.field.next();
            }
            return;
        }
        self.focus = match self.focus {
            Focus::Sidebar => Focus::MessageList,
            Focus::MessageList => Focus::Reader,
            Focus::Reader => Focus::Sidebar,
        };
    }

    fn focus_prev(&mut self) {
        if self.mode == Mode::Compose {
            if let Some(c) = self.compose.as_mut() {
                c.field = c.field.prev();
            }
            return;
        }
        if self.mode == Mode::Onboarding {
            if let Some(o) = self.onboarding.as_mut() {
                o.field = o.field.prev();
            }
            return;
        }
        self.focus = match self.focus {
            Focus::Sidebar => Focus::Reader,
            Focus::MessageList => Focus::Sidebar,
            Focus::Reader => Focus::MessageList,
        };
    }

    fn move_up(&mut self) {
        match self.focus {
            Focus::Sidebar => self.sidebar_up(),
            Focus::MessageList => {
                if self.message_idx > 0 {
                    self.message_idx -= 1;
                    self.refresh_body();
                }
            }
            Focus::Reader => {
                self.reader_scroll = self.reader_scroll.saturating_sub(1);
            }
        }
    }

    fn move_down(&mut self) {
        match self.focus {
            Focus::Sidebar => self.sidebar_down(),
            Focus::MessageList => {
                if self.message_idx + 1 < self.messages.len() {
                    self.message_idx += 1;
                    self.refresh_body();
                }
            }
            Focus::Reader => {
                self.reader_scroll = self.reader_scroll.saturating_add(1);
            }
        }
    }

    fn sidebar_up(&mut self) {
        if self.sidebar_folder_idx > 0 {
            self.sidebar_folder_idx -= 1;
        } else if self.sidebar_account_idx > 0 {
            self.sidebar_account_idx -= 1;
            let folders_len = self.accounts[self.sidebar_account_idx].folders.len();
            self.sidebar_folder_idx = folders_len.saturating_sub(1);
        }
        self.refresh_messages();
    }

    fn sidebar_down(&mut self) {
        let folders_len = self
            .accounts
            .get(self.sidebar_account_idx)
            .map(|a| a.folders.len())
            .unwrap_or(0);
        if self.sidebar_folder_idx + 1 < folders_len {
            self.sidebar_folder_idx += 1;
        } else if self.sidebar_account_idx + 1 < self.accounts.len() {
            self.sidebar_account_idx += 1;
            self.sidebar_folder_idx = 0;
        }
        self.refresh_messages();
    }

    fn page(&mut self, delta: i32) {
        match self.focus {
            Focus::MessageList => {
                let new = (self.message_idx as i32 + delta).clamp(0, self.messages.len().saturating_sub(1) as i32) as usize;
                self.message_idx = new;
                self.refresh_body();
            }
            Focus::Reader => {
                if delta < 0 {
                    self.reader_scroll = self.reader_scroll.saturating_sub((-delta) as u16);
                } else {
                    self.reader_scroll = self.reader_scroll.saturating_add(delta as u16);
                }
            }
            Focus::Sidebar => {
                for _ in 0..delta.unsigned_abs() {
                    if delta > 0 { self.sidebar_down(); } else { self.sidebar_up(); }
                }
            }
        }
    }

    fn goto(&mut self, idx: usize) {
        if self.focus == Focus::MessageList {
            self.message_idx = idx.min(self.messages.len().saturating_sub(1));
            self.refresh_body();
        }
    }

    fn cycle_account(&mut self, delta: i32) {
        let n = self.accounts.len();
        if n == 0 { return; }
        let new = ((self.sidebar_account_idx as i32 + delta).rem_euclid(n as i32)) as usize;
        self.sidebar_account_idx = new;
        self.sidebar_folder_idx = 0;
        self.refresh_messages();
    }

    fn open_message(&mut self) {
        if self.mode == Mode::Search {
            if let Some(first) = self.messages.iter().position(|m| self.search_results.contains(&m.id)) {
                self.message_idx = first;
                self.refresh_body();
            }
            self.mode = Mode::Normal;
            return;
        }
        if let Some(m) = self.current_message() {
            let id = m.id;
            self.data.mark_read(id);
            self.refresh_messages();
            self.focus = Focus::Reader;
        }
    }

    fn back_to_list(&mut self) {
        match self.focus {
            Focus::Reader => self.focus = Focus::MessageList,
            _ => {}
        }
    }

    fn delete_current(&mut self) {
        self.status = "Delete not implemented in mock".into();
    }

    fn toggle_flag(&mut self) {
        if let Some(m) = self.current_message() {
            let id = m.id;
            self.data.toggle_flag(id);
            self.refresh_messages();
        }
    }

    fn mark_read(&mut self) {
        if let Some(m) = self.current_message() {
            let id = m.id;
            self.data.mark_read(id);
            self.refresh_messages();
        }
    }

    fn start_compose_new(&mut self) {
        let acc = match self.current_account() {
            Some(a) => a.clone(),
            None => {
                self.open_onboarding();
                return;
            }
        };
        let draft = empty_draft(acc.id, acc.address.clone());
        let accounts: Vec<Account> = self.accounts.iter().map(|a| a.account.clone()).collect();
        self.compose = Some(ComposeState::from_draft(draft, &accounts));
        self.mode = Mode::Compose;
    }

    fn start_reply(&mut self, all: bool) {
        let acc = match self.current_account() {
            Some(a) => a.clone(),
            None => {
                self.open_onboarding();
                return;
            }
        };
        let msg = match self.current_message() {
            Some(m) => m.clone(),
            None => return,
        };
        let mut msg_with_body = msg.clone();
        msg_with_body.body = self.current_body.clone().or(msg.body.clone());
        let draft = build_reply(&msg_with_body, all, &acc.address);
        let accounts: Vec<Account> = self.accounts.iter().map(|a| a.account.clone()).collect();
        self.compose = Some(ComposeState::from_draft(draft, &accounts));
        self.mode = Mode::Compose;
    }

    fn start_forward(&mut self) {
        let acc = match self.current_account() {
            Some(a) => a.clone(),
            None => {
                self.open_onboarding();
                return;
            }
        };
        let msg = match self.current_message() {
            Some(m) => m.clone(),
            None => return,
        };
        let mut msg_with_body = msg.clone();
        msg_with_body.body = self.current_body.clone().or(msg.body.clone());
        let draft = build_forward(&msg_with_body, &acc.address);
        let accounts: Vec<Account> = self.accounts.iter().map(|a| a.account.clone()).collect();
        self.compose = Some(ComposeState::from_draft(draft, &accounts));
        self.mode = Mode::Compose;
    }

    fn send_compose(&mut self) {
        let accounts: Vec<Account> = self.accounts.iter().map(|a| a.account.clone()).collect();
        let mut taken = match self.compose.take() {
            Some(c) => c,
            None => return,
        };
        taken.sync_to_draft(&accounts);
        match self.data.send(&taken.draft) {
            Ok(()) => {
                self.status = "Sent".into();
                self.mode = Mode::Normal;
            }
            Err(e) => {
                self.last_error = Some(e.to_string());
                self.status = format!("Send failed: {e}");
                self.compose = Some(taken);
            }
        }
    }

    fn save_compose_draft(&mut self) {
        let accounts: Vec<Account> = self.accounts.iter().map(|a| a.account.clone()).collect();
        if let Some(c) = self.compose.as_mut() {
            c.sync_to_draft(&accounts);
            match self.data.save_draft(&c.draft) {
                Ok(()) => self.status = "Draft saved".into(),
                Err(e) => self.status = format!("Save failed: {e}"),
            }
        }
    }

    /// Convenience: account ids in display order.
    #[allow(dead_code)]
    pub fn account_ids(&self) -> Vec<AccountId> {
        self.accounts.iter().map(|a| a.account.id).collect()
    }

    /// Convenience: folder ids for a given account.
    #[allow(dead_code)]
    pub fn folder_ids(&self, account: AccountId) -> Vec<FolderId> {
        self.accounts
            .iter()
            .find(|a| a.account.id == account)
            .map(|a| a.folders.iter().map(|f| f.id).collect())
            .unwrap_or_default()
    }
}
