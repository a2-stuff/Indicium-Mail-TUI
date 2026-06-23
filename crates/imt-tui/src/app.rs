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
        body.set_cursor_line_style(ratatui::style::Style::default());
        body.set_cursor_style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::REVERSED));
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
    /// Password input (rendered masked). Only for password auth.
    pub password: Input,
    /// Whether OAuth2 is selected (vs password auth).
    pub use_oauth2: bool,
    /// OAuth2 client_id input.
    pub client_id: Input,
    /// OAuth2 client_secret input (optional; leave empty for PKCE-only flows).
    pub client_secret: Input,
    /// Authorization code the user pastes after authorizing in browser.
    pub auth_code: Input,
    /// PKCE verifier generated when the auth URL was first built (stable across re-renders).
    pub oauth_pkce_verifier: Option<String>,
    /// CSRF state token round-tripped through the authorization endpoint.
    pub oauth_state: Option<String>,
    /// Generated auth URL shown to the user.
    pub oauth_auth_url: Option<String>,
    /// Redirect URI used in the auth URL.
    pub oauth_redirect_uri: String,
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
            use_oauth2: false,
            client_id: Input::default(),
            client_secret: Input::default(),
            auth_code: Input::default(),
            oauth_pkce_verifier: None,
            oauth_state: None,
            oauth_auth_url: None,
            oauth_redirect_uri: "http://localhost:9876".to_string(),
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
        if self.use_oauth2 {
            let client_id = self.client_id.value().trim().to_string();
            if client_id.is_empty() {
                anyhow::bail!("Client ID is required for OAuth2");
            }
            let code = self.auth_code.value().trim().to_string();
            if code.is_empty() {
                anyhow::bail!("Authorization code is required - complete the browser flow first");
            }
            let verifier = self.oauth_pkce_verifier.clone().unwrap_or_default();
            return Ok(NewAccountForm {
                display_name: self.display_name.value().trim().to_string(),
                email,
                imap_host,
                imap_port,
                imap_tls: self.imap_tls,
                smtp_host,
                smtp_port,
                smtp_tls: self.smtp_tls,
                username,
                password: String::new(),
                oauth_client_id: client_id,
                oauth_client_secret: self.client_secret.value().trim().to_string(),
                oauth_code: code,
                oauth_verifier: verifier,
                oauth_redirect_uri: self.oauth_redirect_uri.clone(),
            });
        }
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
            oauth_client_id: String::new(),
            oauth_client_secret: String::new(),
            oauth_code: String::new(),
            oauth_verifier: String::new(),
            oauth_redirect_uri: String::new(),
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
    /// Tick counter, incremented every 250ms; drives the loading spinner.
    pub ticks: u64,
    /// Latest backend status string from `data.status()`.
    pub backend_status: String,
    /// Tick at which `status` was last set; status auto-clears after `STATUS_TTL_TICKS`.
    pub status_set_tick: u64,
    /// Current runtime settings.
    pub settings: crate::settings::Settings,
    /// Settings modal state, when open.
    pub settings_state: Option<SettingsState>,
    /// Account manager state, when open.
    pub accounts_state: Option<AccountsState>,
    /// Move-to-folder modal state, when open.
    pub move_state: Option<MoveState>,
    /// Tick at which the currently selected message was first viewed by the
    /// reader. Used to mark-as-read after `READ_DELAY_TICKS` ticks.
    pub message_view_started_tick: Option<u64>,
    /// Last message id whose view timer is being tracked.
    pub message_view_id: Option<imt_core::MessageId>,
    /// When set, the next onboarding save is interpreted as edit-of-existing
    /// rather than add-new.
    pub onboarding_edit_id: Option<imt_core::AccountId>,
    /// Tick at which the last auto-refresh fired.
    pub last_auto_refresh_tick: u64,
    /// Hook for the binary to persist settings; called whenever settings change.
    pub on_settings_changed: Option<std::sync::Arc<dyn Fn(&crate::settings::Settings) + Send + Sync>>,
    /// File picker modal state, when open.
    pub file_picker: Option<FilePickerState>,
    /// Attachment viewer modal state, when open.
    pub attachment_viewer: Option<AttachmentViewerState>,
    /// Inline HTML viewer: rendered text content and scroll offset.
    pub html_viewer: Option<(String, u16)>,
    /// Receiver for a pending background AI reply generation, if any.
    pub ai_rx: Option<std::sync::mpsc::Receiver<crate::ai::AiResult>>,
    /// True while an AI reply is being generated in the background.
    pub ai_generating: bool,
}

/// Settings modal state.
pub struct SettingsState {
    pub field: crate::settings::SettingsField,
    pub auto_refresh_secs: tui_input::Input,
    pub browser: tui_input::Input,
    pub ai_model: tui_input::Input,
    pub draft: crate::settings::Settings,
}

impl SettingsState {
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        Self {
            field: crate::settings::SettingsField::AutoRefreshSecs,
            auto_refresh_secs: tui_input::Input::new(s.auto_refresh_secs.to_string()),
            browser: tui_input::Input::new(s.browser.clone()),
            ai_model: tui_input::Input::new(s.ai_model.clone()),
            draft: s.clone(),
        }
    }
}

/// Account manager state.
pub struct AccountsState {
    pub selected: usize,
    pub confirm_delete: Option<imt_core::AccountId>,
}

impl AccountsState {
    pub fn new() -> Self {
        Self { selected: 0, confirm_delete: None }
    }
}

impl Default for AccountsState {
    fn default() -> Self { Self::new() }
}

const STATUS_TTL_TICKS: u64 = 48; // ~12 seconds at 250ms tick
const READ_DELAY_TICKS: u64 = 12; // 3 seconds at 250ms tick

/// Attachment viewer modal state.
pub enum AttachmentViewMode {
    /// Listing attachments to pick one.
    Listing { selected: usize },
    /// Showing the content of a single viewable attachment.
    Viewing { idx: usize, content: String, scroll: u16 },
}

pub struct AttachmentViewerState {
    pub attachments: Vec<imt_core::Attachment>,
    pub mode: AttachmentViewMode,
    /// Download destination chosen by user (for save action).
    pub save_dest: Option<std::path::PathBuf>,
}

/// Move-to-folder modal state.
pub struct MoveState {
    pub message_id: imt_core::MessageId,
    pub folders: Vec<imt_core::Folder>,
    pub selected: usize,
}

/// A single filesystem entry shown in the file picker.
pub struct FileEntry {
    pub name: String,
    pub path: std::path::PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

/// State for the file picker modal.
pub struct FilePickerState {
    pub current_dir: std::path::PathBuf,
    pub entries: Vec<FileEntry>,
    pub selected_idx: usize,
    pub picked: Vec<std::path::PathBuf>,
}

impl FilePickerState {
    pub fn new() -> Self {
        let home = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/"));
        let mut s = Self {
            current_dir: home,
            entries: Vec::new(),
            selected_idx: 0,
            picked: Vec::new(),
        };
        s.reload();
        s
    }

    pub fn reload(&mut self) {
        self.entries = read_dir_entries(&self.current_dir);
        self.selected_idx = 0;
    }
}

fn read_dir_entries(dir: &std::path::Path) -> Vec<FileEntry> {
    let mut entries = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            let meta = entry.metadata().ok();
            let is_dir = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
            let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
            let name = path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("")
                .to_string();
            if name.starts_with('.') { continue; }
            entries.push(FileEntry { name, path, is_dir, size });
        }
    }
    entries.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

pub fn is_viewable_mime(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/typescript"
                | "application/x-sh"
                | "application/x-python"
                | "application/toml"
                | "application/yaml"
                | "application/x-yaml"
                | "application/sql"
                | "application/graphql"
                | "application/ld+json"
        )
}

pub fn is_viewable_by_name(filename: &str) -> bool {
    let ext = std::path::Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        ext.as_str(),
        "txt" | "md" | "markdown" | "log" | "csv" | "json" | "xml" | "yaml" | "yml"
            | "toml" | "html" | "htm" | "css" | "js" | "ts" | "sh" | "bash" | "zsh"
            | "py" | "rb" | "go" | "rs" | "c" | "h" | "cpp" | "java" | "kt" | "swift"
            | "sql" | "graphql" | "ini" | "cfg" | "conf" | "env" | "diff" | "patch"
            | "gitignore" | "dockerfile" | "makefile"
    )
}

pub fn is_viewable(mime: &str, filename: &str) -> bool {
    is_viewable_mime(mime) || is_viewable_by_name(filename)
}

fn mime_for_path(path: &std::path::Path) -> String {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "txt" | "log" => "text/plain",
        "html" | "htm" => "text/html",
        "csv" => "text/csv",
        "json" => "application/json",
        "xml" => "application/xml",
        "zip" => "application/zip",
        "gz" | "tgz" => "application/gzip",
        "tar" => "application/x-tar",
        "mp3" => "audio/mpeg",
        "mp4" => "video/mp4",
        "mkv" => "video/x-matroska",
        "doc" => "application/msword",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xls" => "application/vnd.ms-excel",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "ppt" => "application/vnd.ms-powerpoint",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }.to_string()
}

impl App {
    /// Whether the backend is currently doing work (sync / connect / refresh / fetch).
    pub fn is_busy(&self) -> bool {
        let s = self.backend_status.to_lowercase();
        s.contains("sync") || s.contains("connect") || s.contains("refresh") || s.contains("fetch")
    }
    /// Single-character spinner frame for "loading" states.
    pub fn spinner_frame(&self) -> char {
        const FRAMES: &[char] = &['\u{280B}', '\u{2819}', '\u{2839}', '\u{2838}', '\u{283C}', '\u{2834}', '\u{2826}', '\u{2827}', '\u{2807}', '\u{280F}'];
        FRAMES[(self.ticks as usize) % FRAMES.len()]
    }
    /// Set the transient status line; the message decays automatically.
    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_set_tick = self.ticks;
    }
    /// Clear the transient status line.
    pub fn clear_status(&mut self) {
        self.status.clear();
    }
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
            ticks: 0,
            backend_status: String::new(),
            status_set_tick: 0,
            settings: crate::settings::Settings::default(),
            settings_state: None,
            accounts_state: None,
            move_state: None,
            message_view_started_tick: None,
            message_view_id: None,
            onboarding_edit_id: None,
            last_auto_refresh_tick: 0,
            on_settings_changed: None,
            file_picker: None,
            attachment_viewer: None,
            html_viewer: None,
            ai_rx: None,
            ai_generating: false,
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

    /// Apply a complete `Settings` value to the running app.
    pub fn apply_settings(&mut self, s: crate::settings::Settings) {
        self.html_external = s.html_external;
        self.browser = s.browser.clone();
        crate::theme::apply(s.theme);
        self.settings = s;
    }

    /// Open the settings modal.
    pub fn open_settings(&mut self) {
        self.settings_state = Some(SettingsState::from_settings(&self.settings));
        self.mode = crate::keymap::Mode::Settings;
    }

    /// Open the account manager modal.
    pub fn open_accounts(&mut self) {
        self.accounts_state = Some(AccountsState::new());
        self.mode = crate::keymap::Mode::Accounts;
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

    /// Non-blocking check for a finished background AI reply; inserts it at the
    /// compose body cursor when it arrives.
    fn poll_ai_reply(&mut self) {
        if !self.ai_generating {
            return;
        }
        let received = match self.ai_rx.as_ref() {
            Some(rx) => match rx.try_recv() {
                Ok(result) => Some(Ok(result)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(())),
            },
            None => Some(Err(())),
        };
        let Some(outcome) = received else { return };
        self.ai_generating = false;
        self.ai_rx = None;
        match outcome {
            Ok(Ok(reply)) => {
                if let Some(c) = self.compose.as_mut() {
                    c.field = ComposeField::Body;
                    c.body.insert_str(&reply);
                    self.set_status("AI reply inserted");
                } else {
                    self.set_status("AI reply ready (compose closed)");
                }
            }
            Ok(Err(e)) => self.set_status(format!("AI: {e}")),
            Err(()) => self.set_status("AI generation failed"),
        }
    }

    fn maybe_mark_read_after_dwell(&mut self) {
        if !self.settings.mark_read_on_open {
            return;
        }
        if self.focus != Focus::Reader {
            self.message_view_started_tick = None;
            self.message_view_id = None;
            return;
        }
        let current_id = self.current_message().map(|m| m.id);
        let unread = self.current_message().map(|m| m.is_unread()).unwrap_or(false);
        if !unread || current_id.is_none() {
            self.message_view_started_tick = None;
            self.message_view_id = None;
            return;
        }
        if self.message_view_id != current_id {
            self.message_view_id = current_id;
            self.message_view_started_tick = Some(self.ticks);
            return;
        }
        if let (Some(start), Some(id)) = (self.message_view_started_tick, current_id) {
            if self.ticks.saturating_sub(start) >= READ_DELAY_TICKS {
                self.data.mark_read(id);
                self.message_view_started_tick = None;
            }
        }
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
        self.ticks = self.ticks.wrapping_add(1);
        self.backend_status = self.data.status();
        self.poll_ai_reply();
        // Drain one notification per tick and surface it as a toast.
        if self.status.is_empty() {
            if let Some(notif) = self.data.pop_notification() {
                self.set_status(notif);
            }
        }
        if !self.status.is_empty() && self.ticks.saturating_sub(self.status_set_tick) >= STATUS_TTL_TICKS {
            self.status.clear();
        }
        self.maybe_mark_read_after_dwell();
        if self.settings.auto_refresh_secs > 0 {
            let interval_ticks = (self.settings.auto_refresh_secs as u64) * 4;
            if self.ticks.saturating_sub(self.last_auto_refresh_tick) >= interval_ticks {
                self.last_auto_refresh_tick = self.ticks;
                self.data.refresh(None, None);
            }
        }
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
        if self.mode == Mode::FilePicker {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
            }
            return;
        }
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
                    ComposeField::From => {
                        if matches!(key.code, crossterm::event::KeyCode::Left) && c.field == ComposeField::From {
                            if c.from_idx > 0 { c.from_idx -= 1; }
                        }
                        if matches!(key.code, crossterm::event::KeyCode::Right) && c.field == ComposeField::From {
                            if c.from_idx + 1 < self.accounts.len() { c.from_idx += 1; }
                        }
                    }
                    ComposeField::Attachments => {
                        match key.code {
                            crossterm::event::KeyCode::Delete | crossterm::event::KeyCode::Backspace => {
                                if let Some(c) = self.compose.as_mut() {
                                    if !c.draft.attachments.is_empty() {
                                        c.draft.attachments.pop();
                                    }
                                }
                            }
                            _ => {}
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
        if self.mode == Mode::Settings {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
                return;
            }
            if let Some(s) = self.settings_state.as_mut() {
                use crate::settings::SettingsField;
                let evt = crossterm::event::Event::Key(key);
                match s.field {
                    SettingsField::AutoRefreshSecs => {
                        if accept_numeric(key) { s.auto_refresh_secs.handle_event(&evt); }
                    }
                    SettingsField::Browser => { s.browser.handle_event(&evt); }
                    SettingsField::AiModel => { s.ai_model.handle_event(&evt); }
                    _ => {}
                }
            }
            return;
        }
        if self.mode == Mode::Info {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
            }
            return;
        }
        if self.mode == Mode::Accounts || self.mode == Mode::Move {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
            }
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
                    OnboardingField::ClientId => { o.client_id.handle_event(&evt); }
                    OnboardingField::ClientSecret => { o.client_secret.handle_event(&evt); }
                    OnboardingField::AuthCode => { o.auth_code.handle_event(&evt); }
                    OnboardingField::ImapTls | OnboardingField::SmtpTls | OnboardingField::AuthType => {}
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
        let n = self.search_results.len();
        let msg = if n == 0 { "No matches".to_string() } else { format!("{} match{}", n, if n == 1 { "" } else { "es" }) };
        self.set_status(msg);
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
            KeyAction::EmptyTrash => self.empty_trash_current(),
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
                self.clear_status();
            }
            KeyAction::AddAttachment => {
                let mut picker = FilePickerState::new();
                if let Some(c) = self.compose.as_ref() {
                    picker.picked = c.draft.attachments.iter().map(|a| a.path.clone()).collect();
                }
                self.file_picker = Some(picker);
                self.mode = Mode::FilePicker;
            }
            KeyAction::FilePickerToggle => self.file_picker_toggle(),
            KeyAction::FilePickerParent => self.file_picker_parent(),
            KeyAction::FilePickerConfirm => self.file_picker_confirm(),
            KeyAction::FilePickerCancel => {
                self.file_picker = None;
                self.mode = Mode::Compose;
            }
            KeyAction::OpenOnboarding => self.open_onboarding(),
            KeyAction::SaveOnboarding => self.save_onboarding(),
            KeyAction::CancelOnboarding => {
                self.onboarding = None;
                self.mode = Mode::Normal;
                self.clear_status();
            }
            KeyAction::OnboardingCycleLeft => self.cycle_tls(-1),
            KeyAction::OnboardingCycleRight => self.cycle_tls(1),
            KeyAction::OpenHtmlViewer => self.open_html_viewer(),
            KeyAction::CloseHtmlViewer => {
                self.html_viewer = None;
                self.mode = Mode::Normal;
            }
            KeyAction::Refresh => {
                let acc = self.current_account().map(|a| a.id);
                let folder = self.current_folder().map(|f| f.id);
                self.data.refresh(acc, folder);
                self.set_status("refreshing...");
                self.backend_status = "refreshing".into();
            }
            KeyAction::OpenSettings => self.open_settings(),
            KeyAction::CancelSettings => {
                self.settings_state = None;
                self.mode = Mode::Normal;
            }
            KeyAction::SaveSettings => self.save_settings(),
            KeyAction::SettingsToggle => self.toggle_setting_field(),
            KeyAction::ThemeCycleLeft => self.settings_cycle(-1),
            KeyAction::ThemeCycleRight => self.settings_cycle(1),
            KeyAction::OpenAccounts => self.open_accounts(),
            KeyAction::CloseAccounts => {
                self.accounts_state = None;
                self.mode = Mode::Normal;
            }
            KeyAction::AccountsEdit => self.edit_selected_account(),
            KeyAction::AccountsDelete => self.delete_selected_account(),
            KeyAction::AccountsAdd => {
                self.accounts_state = None;
                self.open_onboarding();
            }
            KeyAction::ToggleRead => self.toggle_read_current(),
            KeyAction::OpenMoveModal => self.open_move_modal(),
            KeyAction::MoveCancel => {
                self.move_state = None;
                self.mode = Mode::Normal;
            }
            KeyAction::MoveSelect => self.move_select(),
            KeyAction::OpenInfo => self.mode = Mode::Info,
            KeyAction::CloseInfo => self.mode = Mode::Normal,
            KeyAction::OpenAttachments => self.open_attachment_viewer(),
            KeyAction::AttachmentView => self.attachment_view_open(),
            KeyAction::AttachmentSave => self.attachment_save(),
            KeyAction::AttachmentClose => self.attachment_close(),
            KeyAction::AiGenerateReply => self.ai_generate_reply(),
        }
    }

    /// Cycle the value of the currently-selected settings field (←/→).
    fn settings_cycle(&mut self, dir: i32) {
        use crate::settings::SettingsField;
        let field = match self.settings_state.as_ref() {
            Some(s) => s.field,
            None => return,
        };
        match field {
            SettingsField::Theme => self.cycle_theme(dir),
            SettingsField::AiProvider => {
                if let Some(state) = self.settings_state.as_mut() {
                    state.draft.ai_provider = if dir >= 0 {
                        state.draft.ai_provider.next()
                    } else {
                        state.draft.ai_provider.prev()
                    };
                }
            }
            _ => {}
        }
    }

    /// Kick off a background AI reply generation for the open compose modal.
    /// Empty body => reply from the thread; non-empty => refine the user's notes
    /// using the thread. Result is inserted at the body cursor when it arrives.
    fn ai_generate_reply(&mut self) {
        if self.ai_generating {
            self.set_status("Already generating...");
            return;
        }
        let compose = match self.compose.as_ref() {
            Some(c) => c,
            None => return,
        };
        let body_text = compose.body.lines().join("\n");
        let (user_notes, quoted) = crate::ai::split_notes_and_quote(&body_text);

        let acc = self.accounts.get(compose.from_idx).map(|a| &a.account);
        let my_name = acc.map(|a| a.display_name.clone()).unwrap_or_default();
        let my_email = acc.map(|a| a.address.email.clone()).unwrap_or_default();
        let subject_fallback = compose.subject.value().to_string();

        let msg = self.current_message().cloned();
        let body = self.current_body.clone();

        let original = body
            .as_ref()
            .map(crate::ai::body_to_text)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| crate::ai::unquote(&quoted));

        let (from, subject, date) = match &msg {
            Some(m) => (
                m.headers.from.first().map(|a| a.format()).unwrap_or_default(),
                m.headers.subject.clone(),
                m.headers.date.format("%Y-%m-%d %H:%M").to_string(),
            ),
            None => (String::new(), subject_fallback, String::new()),
        };

        if original.trim().is_empty() && user_notes.trim().is_empty() {
            self.set_status("Nothing to work from - reply to a message or type some notes first");
            return;
        }

        let ctx = crate::ai::ReplyContext {
            my_name,
            my_email,
            from,
            subject,
            date,
            original,
            user_notes,
        };
        let prompt = crate::ai::build_prompt(&ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        self.ai_rx = Some(rx);
        self.ai_generating = true;
        self.set_status(format!(
            "Generating reply with {}...",
            self.settings.ai_provider.label()
        ));
        crate::ai::spawn_generate(
            self.settings.ai_provider,
            self.settings.ai_model.clone(),
            prompt,
            tx,
        );
    }

    fn file_picker_toggle(&mut self) {
        let s = match self.file_picker.as_mut() { Some(s) => s, None => return };
        let idx = s.selected_idx;
        if let Some(entry) = s.entries.get(idx) {
            if entry.is_dir {
                let new_dir = entry.path.clone();
                s.current_dir = new_dir;
                s.reload();
            } else {
                let path = entry.path.clone();
                if let Some(pos) = s.picked.iter().position(|p| p == &path) {
                    s.picked.remove(pos);
                } else {
                    s.picked.push(path);
                }
            }
        }
    }

    fn file_picker_parent(&mut self) {
        let s = match self.file_picker.as_mut() { Some(s) => s, None => return };
        if let Some(parent) = s.current_dir.parent().map(|p| p.to_path_buf()) {
            s.current_dir = parent;
            s.reload();
        }
    }

    fn file_picker_confirm(&mut self) {
        let picked = match self.file_picker.take() {
            Some(s) => s.picked,
            None => return,
        };
        self.mode = Mode::Compose;
        if let Some(compose) = self.compose.as_mut() {
            compose.draft.attachments.retain(|a| picked.contains(&a.path));
            for path in &picked {
                if compose.draft.attachments.iter().any(|a| &a.path == path) {
                    continue;
                }
                let filename = path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                let mime_type = mime_for_path(path);
                compose.draft.attachments.push(imt_core::draft::DraftAttachment {
                    filename,
                    mime_type,
                    path: path.clone(),
                    size,
                });
            }
        }
        if picked.is_empty() {
            self.set_status("No attachments selected");
        } else {
            self.set_status(format!("{} attachment(s) added", picked.len()));
        }
    }

    fn cycle_tls(&mut self, delta: i32) {
        let o = match self.onboarding.as_mut() {
            Some(o) => o,
            None => return,
        };
        if o.field == OnboardingField::AuthType {
            o.use_oauth2 = !o.use_oauth2;
            return;
        }
        let target = match o.field {
            OnboardingField::ImapTls => &mut o.imap_tls,
            OnboardingField::SmtpTls => &mut o.smtp_tls,
            _ => return,
        };
        let order = [Tls::Implicit, Tls::StartTls, Tls::None];
        let cur = order.iter().position(|t| *t == *target).unwrap_or(0) as i32;
        let next = (cur + delta).rem_euclid(order.len() as i32) as usize;
        *target = order[next];
        match o.field {
            OnboardingField::ImapTls => o.user_edited_imap = true,
            OnboardingField::SmtpTls => o.user_edited_smtp = true,
            _ => {}
        }
    }

    /// Generate the OAuth2 PKCE verifier and auth URL if not already done.
    ///
    /// Delegates to `imt_net::OAuthFlow` so PKCE, state token, and provider
    /// scope/URL handling stay consistent with the rest of the workspace.
    fn ensure_oauth_url_generated(&mut self) {
        let o = match self.onboarding.as_mut() { Some(o) => o, None => return };
        if !o.use_oauth2 || o.oauth_pkce_verifier.is_some() { return; }

        let client_id = o.client_id.value().trim().to_string();
        if client_id.is_empty() { return; }

        let client_secret_raw = o.client_secret.value().trim().to_string();
        let client_secret = if client_secret_raw.is_empty() { None } else { Some(client_secret_raw) };

        // Detect provider from IMAP host; default to Google when unknown.
        let imap_host = o.imap_host.value().to_string();
        let provider = imt_net::OAuthProvider::from_imap_host(&imap_host)
            .unwrap_or(imt_net::OAuthProvider::Google);

        let redirect_uri = o.oauth_redirect_uri.clone();
        let email = o.email.value().trim().to_string();
        let login_hint: Option<&str> = if email.is_empty() { None } else { Some(email.as_str()) };

        let flow = imt_net::OAuthFlow::new(provider, client_id, client_secret);
        let (url, verifier, state) = flow.authorize_url(&redirect_uri, login_hint);
        let url_string = url.to_string();

        o.oauth_pkce_verifier = Some(verifier.0);
        o.oauth_state = Some(state.0);
        o.oauth_auth_url = Some(url_string.clone());

        // Best-effort: open in default browser.
        tokio::spawn(async move {
            let _ = tokio::process::Command::new("xdg-open").arg(&url_string).spawn();
        });
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
        if let Some(edit_id) = self.onboarding_edit_id.take() {
            let pw_changed = !form.password.is_empty();
            match self.data.update_account(edit_id, form, pw_changed) {
                Ok(()) => {
                    self.onboarding = None;
                    self.mode = Mode::Normal;
                    self.set_status("Account updated");
                }
                Err(e) => {
                    self.set_status(format!("Update failed: {e}"));
                    self.last_error = Some(e.to_string());
                }
            }
            return;
        }
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

    fn save_settings(&mut self) {
        let Some(state) = self.settings_state.as_ref() else { return; };
        let mut new_settings = state.draft.clone();
        new_settings.auto_refresh_secs = state
            .auto_refresh_secs
            .value()
            .parse::<u32>()
            .unwrap_or(self.settings.auto_refresh_secs);
        new_settings.browser = state.browser.value().to_string();
        new_settings.ai_model = state.ai_model.value().trim().to_string();
        if let Some(cb) = self.on_settings_changed.clone() {
            cb(&new_settings);
        }
        self.apply_settings(new_settings);
        self.settings_state = None;
        self.mode = Mode::Normal;
        self.set_status("Settings saved");
    }

    fn toggle_setting_field(&mut self) {
        use crate::settings::SettingsField;
        let Some(state) = self.settings_state.as_mut() else { return; };
        match state.field {
            SettingsField::MarkReadOnOpen => state.draft.mark_read_on_open = !state.draft.mark_read_on_open,
            SettingsField::HtmlExternal => state.draft.html_external = !state.draft.html_external,
            SettingsField::ShowSnippet => state.draft.show_snippet = !state.draft.show_snippet,
            SettingsField::Theme => self.cycle_theme(1),
            _ => {}
        }
    }

    fn cycle_theme(&mut self, dir: i32) {
        let Some(state) = self.settings_state.as_mut() else { return; };
        state.draft.theme = if dir >= 0 {
            state.draft.theme.next()
        } else {
            state.draft.theme.prev()
        };
        crate::theme::apply(state.draft.theme);
    }

    fn open_attachment_viewer(&mut self) {
        let attachments = match self.current_body.as_ref() {
            Some(b) if !b.attachments.is_empty() => b.attachments.clone(),
            Some(_) => { self.set_status("No attachments"); return; }
            None => { self.set_status("Open a message first"); return; }
        };
        self.attachment_viewer = Some(AttachmentViewerState {
            attachments,
            mode: AttachmentViewMode::Listing { selected: 0 },
            save_dest: None,
        });
        self.mode = Mode::AttachmentViewer;
    }

    fn attachment_view_open(&mut self) {
        let av = match self.attachment_viewer.as_mut() { Some(s) => s, None => return };
        let idx = match &av.mode {
            AttachmentViewMode::Listing { selected } => *selected,
            AttachmentViewMode::Viewing { .. } => { av.mode = AttachmentViewMode::Listing { selected: 0 }; return; }
        };
        let att = match av.attachments.get(idx) { Some(a) => a.clone(), None => return };
        let content = if let Some(path) = &att.temp_path {
            match std::fs::read(path) {
                Ok(bytes) => {
                    if is_viewable(&att.mime_type, &att.filename) {
                        match String::from_utf8(bytes.clone()) {
                            Ok(s) => s,
                            Err(_) => format!("[Binary content - {} bytes]\nSave with [s] to inspect externally.", bytes.len()),
                        }
                    } else {
                        format!("[Binary file: {}]\nMIME type: {}\nSize: {} bytes\n\nPress [s] to save to Downloads.", att.filename, att.mime_type, att.size)
                    }
                }
                Err(e) => format!("[Error reading attachment: {}]", e),
            }
        } else {
            "[Attachment data not available - try re-opening the message]".to_string()
        };
        av.mode = AttachmentViewMode::Viewing { idx, content, scroll: 0 };
    }

    fn attachment_close(&mut self) {
        match self.attachment_viewer.as_ref().map(|av| &av.mode) {
            Some(AttachmentViewMode::Viewing { .. }) => {
                if let Some(av) = self.attachment_viewer.as_mut() {
                    av.mode = AttachmentViewMode::Listing { selected: 0 };
                }
            }
            _ => {
                self.attachment_viewer = None;
                self.mode = Mode::Normal;
            }
        }
    }

    fn attachment_save(&mut self) {
        let av = match self.attachment_viewer.as_ref() { Some(s) => s, None => return };
        let idx = match &av.mode {
            AttachmentViewMode::Listing { selected } => *selected,
            AttachmentViewMode::Viewing { idx, .. } => *idx,
        };
        let att = match av.attachments.get(idx) { Some(a) => a.clone(), None => return };
        let src = match &att.temp_path {
            Some(p) => p.clone(),
            None => { self.set_status("Attachment data not available"); return; }
        };
        let downloads = directories::UserDirs::new()
            .and_then(|u| u.download_dir().map(|p| p.to_path_buf()))
            .or_else(|| directories::UserDirs::new().map(|u| u.home_dir().join("Downloads")))
            .unwrap_or_else(|| std::path::PathBuf::from("."));
        let dest = downloads.join(&att.filename);
        match std::fs::copy(&src, &dest) {
            Ok(_) => self.set_status(format!("Saved to {}", dest.display())),
            Err(e) => self.set_status(format!("Save failed: {}", e)),
        }
    }

    fn edit_selected_account(&mut self) {
        let Some(state) = self.accounts_state.as_ref() else { return; };
        let Some(av) = self.accounts.get(state.selected) else { return; };
        let acc = &av.account;
        let mut form = OnboardingState::new();
        form.display_name = tui_input::Input::new(acc.display_name.clone());
        form.email = tui_input::Input::new(acc.address.email.clone());
        form.imap_host = tui_input::Input::new(acc.imap.host.clone());
        form.imap_port = tui_input::Input::new(acc.imap.port.to_string());
        form.imap_tls = acc.imap.tls;
        form.smtp_host = tui_input::Input::new(acc.smtp.host.clone());
        form.smtp_port = tui_input::Input::new(acc.smtp.port.to_string());
        form.smtp_tls = acc.smtp.tls;
        let user = match &acc.imap.auth {
            imt_core::AuthMethod::Password { username } => username.clone(),
            imt_core::AuthMethod::OAuth2 { username, .. } => username.clone(),
        };
        form.username = tui_input::Input::new(user);
        form.user_edited_imap = true;
        form.user_edited_smtp = true;
        self.onboarding_edit_id = Some(acc.id);
        self.onboarding = Some(form);
        self.accounts_state = None;
        self.mode = Mode::Onboarding;
    }

    fn delete_selected_account(&mut self) {
        let pending_id = self.accounts_state.as_ref().and_then(|s| s.confirm_delete);
        if let Some(id) = pending_id {
            let result = self.data.delete_account(id);
            if let Some(s) = self.accounts_state.as_mut() {
                s.confirm_delete = None;
            }
            match result {
                Ok(()) => self.set_status("Account deleted"),
                Err(e) => self.set_status(format!("Delete failed: {e}")),
            }
            return;
        }
        let target_id = self
            .accounts_state
            .as_ref()
            .and_then(|s| self.accounts.get(s.selected).map(|av| av.account.id));
        if let (Some(id), Some(s)) = (target_id, self.accounts_state.as_mut()) {
            s.confirm_delete = Some(id);
        }
    }

    fn open_html_viewer(&mut self) {
        let html = match self.current_body.as_ref().and_then(|b| b.text_html.clone()) {
            Some(h) => h,
            None => {
                self.set_status("No HTML body in this message");
                return;
            }
        };
        let content = match html2text::from_read(html.as_bytes(), 120) {
            Ok(rendered) => rendered,
            Err(_) => html,
        };
        self.html_viewer = Some((content, 0));
        self.mode = Mode::HtmlViewer;
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
                let oauth2 = o.use_oauth2;
                // Generate auth URL when the user first reaches AuthCode.
                let next = o.field.next(oauth2);
                if next == OnboardingField::AuthCode {
                    self.ensure_oauth_url_generated();
                }
                if let Some(o) = self.onboarding.as_mut() { o.field = next; }
            }
            return;
        }
        if self.mode == Mode::Settings {
            if let Some(s) = self.settings_state.as_mut() {
                s.field = s.field.next();
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
                let oauth2 = o.use_oauth2;
                o.field = o.field.prev(oauth2);
            }
            return;
        }
        if self.mode == Mode::Settings {
            if let Some(s) = self.settings_state.as_mut() {
                s.field = s.field.prev();
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
        if self.mode == Mode::HtmlViewer {
            if let Some((_, scroll)) = self.html_viewer.as_mut() {
                *scroll = scroll.saturating_sub(1);
            }
            return;
        }
        if self.mode == Mode::AttachmentViewer {
            if let Some(av) = self.attachment_viewer.as_mut() {
                match &mut av.mode {
                    AttachmentViewMode::Listing { selected } => { if *selected > 0 { *selected -= 1; } }
                    AttachmentViewMode::Viewing { scroll, .. } => { *scroll = scroll.saturating_sub(1); }
                }
            }
            return;
        }
        if self.mode == Mode::FilePicker {
            if let Some(s) = self.file_picker.as_mut() {
                if s.selected_idx > 0 { s.selected_idx -= 1; }
            }
            return;
        }
        if self.mode == Mode::Move {
            if let Some(s) = self.move_state.as_mut() {
                if s.selected > 0 { s.selected -= 1; }
            }
            return;
        }
        if self.mode == Mode::Accounts {
            if let Some(s) = self.accounts_state.as_mut() {
                if s.selected > 0 { s.selected -= 1; }
                s.confirm_delete = None;
            }
            return;
        }
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
        if self.mode == Mode::HtmlViewer {
            if let Some((_, scroll)) = self.html_viewer.as_mut() {
                *scroll = scroll.saturating_add(1);
            }
            return;
        }
        if self.mode == Mode::AttachmentViewer {
            if let Some(av) = self.attachment_viewer.as_mut() {
                match &mut av.mode {
                    AttachmentViewMode::Listing { selected } => {
                        if *selected + 1 < av.attachments.len() { *selected += 1; }
                    }
                    AttachmentViewMode::Viewing { scroll, .. } => { *scroll = scroll.saturating_add(1); }
                }
            }
            return;
        }
        if self.mode == Mode::FilePicker {
            if let Some(s) = self.file_picker.as_mut() {
                if s.selected_idx + 1 < s.entries.len() { s.selected_idx += 1; }
            }
            return;
        }
        if self.mode == Mode::Move {
            if let Some(s) = self.move_state.as_mut() {
                if s.selected + 1 < s.folders.len() { s.selected += 1; }
            }
            return;
        }
        if self.mode == Mode::Accounts {
            if let Some(s) = self.accounts_state.as_mut() {
                if s.selected + 1 < self.accounts.len() { s.selected += 1; }
                s.confirm_delete = None;
            }
            return;
        }
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
        if self.current_message().is_some() {
            self.focus = Focus::Reader;
            self.refresh_body();
        }
    }

    fn back_to_list(&mut self) {
        match self.focus {
            Focus::Reader => self.focus = Focus::MessageList,
            _ => {}
        }
    }

    fn delete_current(&mut self) {
        let id = match self.current_message() {
            Some(m) => m.id,
            None => return,
        };
        match self.data.delete_message(id) {
            Ok(()) => {
                self.set_status("Moved to Trash");
                self.refresh_messages();
            }
            Err(e) => self.set_status(format!("Delete failed: {e}")),
        }
    }

    fn empty_trash_current(&mut self) {
        let folder = match self.current_folder() {
            Some(f) => f.clone(),
            None => return,
        };
        if folder.role != imt_core::FolderRole::Trash {
            self.set_status("Empty Trash only works inside the Trash folder");
            return;
        }
        match self.data.empty_trash(folder.id) {
            Ok(()) => {
                self.set_status("Emptying Trash...");
                self.refresh_messages();
            }
            Err(e) => self.set_status(format!("Empty Trash failed: {e}")),
        }
    }

    fn toggle_read_current(&mut self) {
        let m = match self.current_message() {
            Some(m) => m,
            None => return,
        };
        let id = m.id;
        let now_seen = !m.is_unread();
        self.data.set_seen(id, !now_seen);
        self.refresh_messages();
        self.set_status(if now_seen { "Marked unread" } else { "Marked read" });
    }

    fn open_move_modal(&mut self) {
        let id = match self.current_message() {
            Some(m) => m.id,
            None => return,
        };
        let acc = match self.current_account() {
            Some(a) => a.id,
            None => return,
        };
        let current_folder = self.current_folder().map(|f| f.id);
        let folders: Vec<_> = self
            .data
            .folders(acc)
            .into_iter()
            .filter(|f| Some(f.id) != current_folder)
            .collect();
        if folders.is_empty() {
            self.set_status("No other folders");
            return;
        }
        self.move_state = Some(MoveState { message_id: id, folders, selected: 0 });
        self.mode = Mode::Move;
    }

    fn move_select(&mut self) {
        let dest = match self.move_state.as_ref() {
            Some(s) => s.folders.get(s.selected).map(|f| (s.message_id, f.id)),
            None => None,
        };
        if let Some((mid, fid)) = dest {
            match self.data.move_message(mid, fid) {
                Ok(()) => {
                    self.set_status("Moved");
                    self.refresh_messages();
                }
                Err(e) => self.set_status(format!("Move failed: {e}")),
            }
        }
        self.move_state = None;
        self.mode = Mode::Normal;
    }

    fn toggle_flag(&mut self) {
        if let Some(m) = self.current_message() {
            let id = m.id;
            let was_flagged = m.is_flagged();
            self.data.toggle_flag(id);
            self.refresh_messages();
            self.set_status(if was_flagged { "Unmarked as important" } else { "★ Marked as important" });
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
