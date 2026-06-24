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

/// How the compose modal is currently being dragged with the mouse.
#[derive(Debug, Clone, Copy)]
pub enum ComposeDrag {
    /// Moving the whole window; offsets are cursor-minus-origin at grab time.
    Move { off_x: u16, off_y: u16 },
    /// Resizing from the bottom-right corner.
    Resize,
}

/// Build the body editor textarea with the shared cursor styling.
fn build_body_textarea(text: &str) -> TextArea<'static> {
    let mut body = TextArea::new(text.lines().map(String::from).collect());
    body.set_block(
        ratatui::widgets::Block::default()
            .borders(ratatui::widgets::Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title("Body"),
    );
    body.set_cursor_line_style(ratatui::style::Style::default());
    body.set_cursor_style(ratatui::style::Style::default().add_modifier(ratatui::style::Modifier::REVERSED));
    body
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
    /// Explicit modal geometry once moved/resized; None = default centered.
    pub area: Option<ratatui::layout::Rect>,
    /// Active mouse drag, if any.
    pub drag: Option<ComposeDrag>,
    /// Last rendered body inner width, used for hard-wrapping.
    pub wrap_width: u16,
    /// When Some, the "Instruction or Context" dialog is open (Ctrl-T).
    pub instruction: Option<Input>,
}

impl ComposeState {
    fn from_draft(draft: Draft, accounts: &[Account]) -> Self {
        let to = Input::new(addr_join(&draft.to));
        let cc = Input::new(addr_join(&draft.cc));
        let bcc = Input::new(addr_join(&draft.bcc));
        let subject = Input::new(draft.subject.clone());
        let body = build_body_textarea(&draft.body_text);
        let from_idx = accounts.iter().position(|a| a.id == draft.account_id).unwrap_or(0);
        Self {
            draft,
            field: ComposeField::To,
            to,
            cc,
            bcc,
            subject,
            body,
            from_idx,
            area: None,
            drag: None,
            wrap_width: 0,
            instruction: None,
        }
    }

    /// Replace the body text (rebuilds the editor), cursor at the top.
    pub fn set_body_text(&mut self, text: &str) {
        self.body = build_body_textarea(text);
    }

    /// Effective wrap width (falls back to 72 if the body hasn't been laid out).
    fn effective_wrap_width(&self) -> usize {
        let w = self.wrap_width as usize;
        if w >= 8 {
            w
        } else {
            72
        }
    }

    /// Hard-wrap the body to the wrap width, breaking over-long lines at word
    /// boundaries while preserving the user's own line breaks (signatures, etc.).
    /// Used on resize / AI insert / send. Cursor returns to the top.
    pub fn rewrap_body(&mut self) {
        let w = self.effective_wrap_width();
        let text = self.body.lines().join("\n");
        let wrapped = crate::ai::break_long_lines(&text, w);
        if wrapped != text {
            self.set_body_text(&wrapped);
        }
    }

    /// Live wrap while typing: break over-long lines at word boundaries without
    /// reflowing the rest, and keep the cursor where the user is editing.
    pub fn wrap_live(&mut self) {
        let w = self.effective_wrap_width();
        let text = self.body.lines().join("\n");
        let wrapped = crate::ai::break_long_lines(&text, w);
        if wrapped == text {
            return; // nothing exceeded the width
        }
        // Preserve the caret by absolute character offset (break_long_lines only
        // swaps a space for a newline, so total length and indices are stable).
        let (row, col) = self.body.cursor();
        let lines = self.body.lines();
        let mut off = col;
        for r in 0..row {
            off += lines[r].chars().count() + 1;
        }
        let mut body = build_body_textarea(&wrapped);
        // Map the offset back to (row, col) in the wrapped text.
        let mut rem = off;
        let mut target_row = 0usize;
        for (idx, l) in wrapped.split('\n').enumerate() {
            let lc = l.chars().count();
            target_row = idx;
            if rem <= lc {
                break;
            }
            rem -= lc + 1;
        }
        use tui_textarea::CursorMove;
        body.move_cursor(CursorMove::Top);
        body.move_cursor(CursorMove::Head);
        for _ in 0..target_row {
            body.move_cursor(CursorMove::Down);
        }
        body.move_cursor(CursorMove::Head);
        for _ in 0..rem {
            body.move_cursor(CursorMove::Forward);
        }
        self.body = body;
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
    /// Leave a copy of downloaded messages on the server (default true).
    pub keep_on_server: bool,
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
            keep_on_server: true,
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
                keep_on_server: self.keep_on_server,
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
            keep_on_server: self.keep_on_server,
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
    /// Interactive menu/actions-bar navigation state, when in `Mode::Menu`.
    pub menu_state: Option<crate::menu::MenuState>,
    /// Width fraction of the sidebar (accounts) pane.
    pub sidebar_frac: f32,
    /// Width fraction of the message-list (inbox) pane.
    pub list_frac: f32,
    /// Last full frame rect (set during draw, used for mouse hit-testing).
    pub ui_frame: ratatui::layout::Rect,
    /// Last three-pane body rect (set during draw).
    pub ui_main: ratatui::layout::Rect,
    /// Last menu-bar / pane rects (set during draw, used for mouse hit-testing).
    pub ui_menu: ratatui::layout::Rect,
    pub ui_sidebar: ratatui::layout::Rect,
    pub ui_list: ratatui::layout::Rect,
    pub ui_reader: ratatui::layout::Rect,
    /// Active pane-divider drag (1 = sidebar|list, 2 = list|reader).
    pub pane_drag: Option<u8>,
    /// Thread view state, when open.
    pub thread_state: Option<ThreadState>,
    /// Number of messages in the current message's thread (>=1); drives the
    /// reader's thread hint. 0 = not computed / no message.
    pub current_thread_count: usize,
}

/// What a sidebar row points at, for mouse hit-testing.
enum SidebarTarget {
    /// Account header at this index.
    Account(usize),
    /// Folder (account index, folder index).
    Folder(usize, usize),
}

/// Thread (conversation) view modal state.
pub struct ThreadState {
    pub messages: Vec<Message>,
    pub selected: usize,
    pub scroll: u16,
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
    /// Showing the extracted text of a single attachment (text / PDF / DOCX).
    Viewing { idx: usize, content: String, scroll: u16 },
    /// Showing a decoded image rendered as half-block cells.
    ViewingImage { idx: usize, image: std::sync::Arc<image::DynamicImage> },
}

pub struct AttachmentViewerState {
    pub attachments: Vec<imt_core::Attachment>,
    pub mode: AttachmentViewMode,
    /// Download destination chosen by user (for save action).
    pub save_dest: Option<std::path::PathBuf>,
    /// Mode to return to when the viewer is fully closed (Normal, or Thread when
    /// opened from the conversation view).
    pub return_mode: Mode,
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
            menu_state: None,
            sidebar_frac: 0.20,
            list_frac: 0.35,
            ui_frame: ratatui::layout::Rect::default(),
            ui_main: ratatui::layout::Rect::default(),
            ui_menu: ratatui::layout::Rect::default(),
            ui_sidebar: ratatui::layout::Rect::default(),
            ui_list: ratatui::layout::Rect::default(),
            ui_reader: ratatui::layout::Rect::default(),
            pane_drag: None,
            thread_state: None,
            current_thread_count: 0,
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
        self.sidebar_frac = s.sidebar_frac.clamp(0.1, 0.6);
        self.list_frac = s.list_frac.clamp(0.1, 0.7);
        self.settings = s;
    }

    /// Persist layout preferences (pane fractions + compose geometry) to disk.
    fn save_layout_prefs(&mut self) {
        self.settings.sidebar_frac = self.sidebar_frac;
        self.settings.list_frac = self.list_frac;
        if let Some(area) = self.compose.as_ref().and_then(|c| c.area) {
            self.settings.compose_geom = Some(crate::settings::WindowGeom::from_rect(area));
        }
        if let Some(cb) = self.on_settings_changed.clone() {
            cb(&self.settings);
        }
    }

    /// Install a compose modal, restoring the saved window geometry if any.
    fn install_compose(&mut self, mut state: ComposeState) {
        if let Some(g) = self.settings.compose_geom {
            state.area = Some(g.to_rect());
        }
        self.compose = Some(state);
        self.mode = Mode::Compose;
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
                    // Replace the user's typed notes with the generated reply so the
                    // notes aren't duplicated; keep the quoted thread below it.
                    let body_text = c.body.lines().join("\n");
                    let (_notes, quoted) = crate::ai::split_notes_and_quote(&body_text);
                    let mut new_body = reply.text.trim().to_string();
                    if !quoted.trim().is_empty() {
                        new_body.push_str("\n\n");
                        new_body.push_str(quoted.trim_start_matches('\n'));
                    }
                    c.field = ComposeField::Body;
                    c.set_body_text(&new_body);
                    c.rewrap_body();
                    // Attach any files the model generated.
                    let mut added = 0;
                    for path in &reply.attachments {
                        if c.draft.attachments.iter().any(|a| &a.path == path) {
                            continue;
                        }
                        let filename = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("file")
                            .to_string();
                        let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
                        let mime_type = mime_for_path(path);
                        c.draft.attachments.push(imt_core::draft::DraftAttachment {
                            filename,
                            mime_type,
                            path: path.clone(),
                            size,
                        });
                        added += 1;
                    }
                    if added > 0 {
                        self.set_status(format!("AI reply inserted ({added} file(s) attached)"));
                    } else {
                        self.set_status("AI reply inserted");
                    }
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
        let cur = self.current_message().map(|m| m.id);
        self.current_thread_count = cur.map(|id| self.data.thread(id).len()).unwrap_or(0);
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
        if self.mode == Mode::Menu {
            self.handle_menu_key(key);
            return;
        }
        if self.mode == Mode::FilePicker {
            if let Some(action) = map_key(self.focus, self.mode, key) {
                self.dispatch(action);
            }
            return;
        }
        if self.mode == Mode::Compose {
            // The "Instruction or Context" dialog captures all input while open.
            let dialog_open = self.compose.as_ref().map(|c| c.instruction.is_some()).unwrap_or(false);
            if dialog_open {
                match key.code {
                    crossterm::event::KeyCode::Esc => {
                        if let Some(c) = self.compose.as_mut() {
                            c.instruction = None;
                        }
                        self.set_status("Instruction cancelled");
                    }
                    crossterm::event::KeyCode::Enter => {
                        let instr = self
                            .compose
                            .as_mut()
                            .and_then(|c| c.instruction.take())
                            .map(|i| i.value().to_string())
                            .unwrap_or_default();
                        self.ai_generate_reply(instr);
                    }
                    _ => {
                        if let Some(inp) = self.compose.as_mut().and_then(|c| c.instruction.as_mut()) {
                            inp.handle_event(&crossterm::event::Event::Key(key));
                        }
                    }
                }
                return;
            }
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
                    ComposeField::Body => { c.body.input(key); c.wrap_live(); }
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
                    OnboardingField::KeepOnServer => {
                        // Space toggles the checkbox (←/→ also cycle it).
                        if matches!(key.code, crossterm::event::KeyCode::Char(' ')) {
                            o.keep_on_server = !o.keep_on_server;
                        }
                    }
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
            KeyAction::AiGenerateReply => self.ai_generate_reply(String::new()),
            KeyAction::AiReplyWithInstructions => {
                if let Some(c) = self.compose.as_mut() {
                    c.instruction = Some(Input::default());
                }
            }
            KeyAction::OpenMenu => self.open_menu(),
            KeyAction::OpenThread => self.open_thread(),
            KeyAction::CloseThread => {
                self.thread_state = None;
                self.mode = Mode::Normal;
            }
        }
    }

    /// Open the thread (conversation) view for the selected message.
    fn open_thread(&mut self) {
        let id = match self.current_message() {
            Some(m) => m.id,
            None => return,
        };
        let messages = self.data.thread(id);
        if messages.len() <= 1 {
            self.set_status("No other messages in this conversation");
            return;
        }
        let selected = messages.iter().position(|m| m.id == id).unwrap_or(0);
        self.thread_state = Some(ThreadState { messages, selected, scroll: 0 });
        self.mode = Mode::Thread;
    }

    /// Handle a raw mouse event (drag panes in normal mode, move/resize the
    /// compose window in compose mode).
    pub fn handle_mouse(&mut self, m: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        if self.mode == Mode::Compose {
            self.handle_compose_mouse(m);
            return;
        }
        match m.kind {
            MouseEventKind::ScrollUp => self.scroll_at(m.column, m.row, -3),
            MouseEventKind::ScrollDown => self.scroll_at(m.column, m.row, 3),
            MouseEventKind::Down(MouseButton::Left) => {
                // Menu bar / open dropdown take priority (works from any non-modal
                // mode so the bar is clickable even while reading).
                if self.mouse_menu_down(m.column, m.row) {
                    return;
                }
                // A click on a pane boundary starts a divider drag; otherwise it
                // selects whatever was clicked in the panes.
                if let Some(d) = self.divider_hit(m.column, m.row) {
                    self.pane_drag = Some(d);
                } else if self.mode == Mode::Normal {
                    self.mouse_click(m.column, m.row);
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(d) = self.pane_drag {
                    self.drag_divider(d, m.column);
                }
            }
            MouseEventKind::Up(_) => {
                if self.pane_drag.take().is_some() {
                    self.save_layout_prefs();
                }
            }
            _ => {}
        }
    }

    /// True if the point is inside `r`.
    fn rect_contains(r: ratatui::layout::Rect, col: u16, row: u16) -> bool {
        col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height
    }

    /// Handle a left-press on the menu bar or an open dropdown. Returns true if
    /// the event was consumed (a menu was opened, an item run, or a menu closed).
    fn mouse_menu_down(&mut self, col: u16, row: u16) -> bool {
        use crate::menu::MENUS;
        // If a dropdown is open, a click inside it runs the item; a click
        // anywhere else (except switching menus, handled below) closes it.
        if self.mode == Mode::Menu {
            if let Some(ms) = self.menu_state {
                if ms.open {
                    if let Some(area) = crate::ui::menubar::dropdown_rect(self.ui_menu, self.ui_frame, ms.col) {
                        // Inner item rows sit one cell inside the bordered box.
                        let inner_top = area.y + 1;
                        let inner_bot = area.y + area.height.saturating_sub(1);
                        if col > area.x && col < area.x + area.width.saturating_sub(1)
                            && row >= inner_top && row < inner_bot
                        {
                            let idx = (row - inner_top) as usize;
                            if let Some(item) = MENUS.get(ms.col).and_then(|mn| mn.items.get(idx)) {
                                let action = item.action;
                                self.menu_state = None;
                                self.mode = Mode::Normal;
                                self.dispatch(action);
                                return true;
                            }
                        }
                    }
                }
            }
        }

        // A click on the menu-bar row itself.
        if row == self.ui_menu.y {
            if let Some(idx) = self.menu_hit(col) {
                let menu = &MENUS[idx];
                if menu.items.is_empty() {
                    if let Some(a) = menu.action {
                        self.menu_state = None;
                        self.mode = Mode::Normal;
                        self.dispatch(a);
                    }
                } else {
                    // Toggle this menu's dropdown.
                    let already = self.mode == Mode::Menu
                        && self.menu_state.map(|m| m.col == idx && m.open).unwrap_or(false);
                    if already {
                        self.menu_state = None;
                        self.mode = Mode::Normal;
                    } else {
                        self.menu_state = Some(crate::menu::MenuState { col: idx, open: true, item: 0 });
                        self.mode = Mode::Menu;
                    }
                }
                return true;
            }
            // Clicked an empty part of the bar - close any open menu.
            if self.mode == Mode::Menu {
                self.menu_state = None;
                self.mode = Mode::Normal;
            }
            return true;
        }

        // Click elsewhere while a menu is open closes it (click-away).
        if self.mode == Mode::Menu {
            self.menu_state = None;
            self.mode = Mode::Normal;
            return true;
        }
        false
    }

    /// Which top menu (if any) sits under column `col` on the menu bar.
    fn menu_hit(&self, col: u16) -> Option<usize> {
        use crate::menu::MENUS;
        let mut x = self.ui_menu.x;
        for (i, m) in MENUS.iter().enumerate() {
            let caret = if m.items.is_empty() { 0 } else { 2 };
            let w = m.label.chars().count() as u16 + 2 + caret;
            if col >= x && col < x + w {
                return Some(i);
            }
            x += w;
        }
        None
    }

    /// Handle a left-click in the panes (sidebar / list / reader).
    fn mouse_click(&mut self, col: u16, row: u16) {
        if Self::rect_contains(self.ui_sidebar, col, row) {
            match self.sidebar_target_at(row) {
                Some(SidebarTarget::Account(ai)) => {
                    self.sidebar_account_idx = ai;
                    if let Some(av) = self.accounts.get_mut(ai) {
                        av.expanded = !av.expanded;
                    }
                    self.focus = Focus::Sidebar;
                }
                Some(SidebarTarget::Folder(ai, fi)) => {
                    self.sidebar_account_idx = ai;
                    self.sidebar_folder_idx = fi;
                    self.message_idx = 0;
                    self.focus = Focus::MessageList;
                    self.refresh_messages();
                }
                None => {}
            }
        } else if Self::rect_contains(self.ui_list, col, row) {
            if let Some(i) = self.message_at_row(row) {
                self.message_idx = i;
                self.focus = Focus::Reader;
                self.refresh_body();
            }
        } else if Self::rect_contains(self.ui_reader, col, row) {
            self.focus = Focus::Reader;
        }
    }

    /// Scroll the pane under the cursor by `delta` (lines/messages, sign = dir).
    fn scroll_at(&mut self, col: u16, row: u16, delta: i32) {
        if self.mode != Mode::Normal {
            return;
        }
        if Self::rect_contains(self.ui_reader, col, row) {
            if delta < 0 {
                self.reader_scroll = self.reader_scroll.saturating_sub((-delta) as u16);
            } else {
                self.reader_scroll = self.reader_scroll.saturating_add(delta as u16);
            }
        } else if Self::rect_contains(self.ui_list, col, row) {
            if self.messages.is_empty() {
                return;
            }
            let new = (self.message_idx as i32 + delta)
                .clamp(0, self.messages.len().saturating_sub(1) as i32) as usize;
            if new != self.message_idx {
                self.message_idx = new;
                self.refresh_body();
            }
        } else if Self::rect_contains(self.ui_sidebar, col, row) {
            for _ in 0..delta.unsigned_abs() {
                if delta > 0 {
                    self.sidebar_down();
                } else {
                    self.sidebar_up();
                }
            }
        }
    }

    /// Map a screen row in the sidebar to an account header or a folder,
    /// mirroring the sidebar renderer's line order.
    fn sidebar_target_at(&self, row: u16) -> Option<SidebarTarget> {
        let inner_top = self.ui_sidebar.y + 1; // inside the top border
        if row < inner_top {
            return None;
        }
        let mut r = inner_top;
        for (ai, av) in self.accounts.iter().enumerate() {
            if row == r {
                return Some(SidebarTarget::Account(ai));
            }
            r += 1;
            if av.expanded {
                for fi in 0..av.folders.len() {
                    if row == r {
                        return Some(SidebarTarget::Folder(ai, fi));
                    }
                    r += 1;
                }
            }
        }
        None
    }

    /// Map a screen row in the message list to a message index, accounting for
    /// the optional 2-line snippet rows.
    fn message_at_row(&self, row: u16) -> Option<usize> {
        let inner_top = self.ui_list.y + 1; // inside the top border
        if row < inner_top {
            return None;
        }
        let show_snippet = self.settings.show_snippet;
        let mut r = inner_top;
        for (i, m) in self.messages.iter().enumerate() {
            let h = if show_snippet && !m.snippet.is_empty() { 2 } else { 1 };
            if row >= r && row < r + h {
                return Some(i);
            }
            r += h;
        }
        None
    }

    /// Which pane divider (1 = sidebar|list, 2 = list|reader) is at this cell.
    fn divider_hit(&self, col: u16, row: u16) -> Option<u8> {
        let main = self.ui_main;
        if main.width == 0 || row < main.y || row >= main.y + main.height {
            return None;
        }
        let (sw, lw, _) = crate::ui::layout::pane_widths(main.width, self.sidebar_frac, self.list_frac);
        let div1 = main.x + sw;
        let div2 = main.x + sw + lw;
        // 1-cell tolerance on either side of the boundary.
        if col + 1 >= div1 && col <= div1 + 1 {
            Some(1)
        } else if col + 1 >= div2 && col <= div2 + 1 {
            Some(2)
        } else {
            None
        }
    }

    fn drag_divider(&mut self, divider: u8, col: u16) {
        let main = self.ui_main;
        if main.width < 24 {
            return;
        }
        let total = main.width as f32;
        match divider {
            1 => {
                let w = col.saturating_sub(main.x).clamp(8, main.width.saturating_sub(16));
                self.sidebar_frac = w as f32 / total;
            }
            2 => {
                let (sw, _, _) = crate::ui::layout::pane_widths(main.width, self.sidebar_frac, self.list_frac);
                let lw = col
                    .saturating_sub(main.x + sw)
                    .clamp(8, main.width.saturating_sub(sw + 8));
                self.list_frac = lw as f32 / total;
            }
            _ => {}
        }
    }

    /// Handle a mouse event while the compose modal is open: drag the title bar
    /// to move, drag the bottom-right corner to resize (re-wrapping the body).
    fn handle_compose_mouse(&mut self, m: crossterm::event::MouseEvent) {
        use crossterm::event::{MouseButton, MouseEventKind};
        let frame = self.ui_frame;
        let area = match self.compose.as_ref().and_then(|c| c.area) {
            Some(a) => a,
            None => return,
        };
        let (col, row) = (m.column, m.row);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let on_corner = col + 1 >= area.x + area.width && col < area.x + area.width
                    && row + 1 >= area.y + area.height && row < area.y + area.height;
                let in_x = col >= area.x && col < area.x + area.width;
                if on_corner {
                    if let Some(c) = self.compose.as_mut() {
                        c.drag = Some(ComposeDrag::Resize);
                    }
                } else if row == area.y && in_x {
                    if let Some(c) = self.compose.as_mut() {
                        c.drag = Some(ComposeDrag::Move { off_x: col - area.x, off_y: row - area.y });
                    }
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let drag = self.compose.as_ref().and_then(|c| c.drag);
                match drag {
                    Some(ComposeDrag::Move { off_x, off_y }) => {
                        let max_x = frame.width.saturating_sub(area.width);
                        let max_y = frame.height.saturating_sub(area.height + 1);
                        let nx = col.saturating_sub(off_x).min(max_x);
                        let ny = row.saturating_sub(off_y).max(1).min(max_y.max(1));
                        if let Some(c) = self.compose.as_mut() {
                            c.area = Some(ratatui::layout::Rect { x: nx, y: ny, ..area });
                        }
                    }
                    Some(ComposeDrag::Resize) => {
                        let new_w = (col.saturating_sub(area.x) + 1)
                            .clamp(34, frame.width.saturating_sub(area.x).max(34));
                        let new_h = (row.saturating_sub(area.y) + 1)
                            .clamp(12, frame.height.saturating_sub(area.y).max(12));
                        if let Some(c) = self.compose.as_mut() {
                            c.area = Some(ratatui::layout::Rect { width: new_w, height: new_h, ..area });
                            c.rewrap_body();
                        }
                    }
                    None => {}
                }
            }
            MouseEventKind::Up(_) => {
                let was_dragging = if let Some(c) = self.compose.as_mut() {
                    if matches!(c.drag, Some(ComposeDrag::Resize)) {
                        c.rewrap_body();
                    }
                    c.drag.take().is_some()
                } else {
                    false
                };
                if was_dragging {
                    self.save_layout_prefs();
                }
            }
            _ => {}
        }
    }

    /// Open the interactive menu bar (Mode::Menu).
    fn open_menu(&mut self) {
        self.menu_state = Some(crate::menu::MenuState::new());
        self.mode = Mode::Menu;
    }

    /// Handle a key while the menu bar is active.
    fn handle_menu_key(&mut self, key: KeyEvent) {
        use crate::menu::MENUS;
        use crossterm::event::KeyCode;
        let mut ms = match self.menu_state {
            Some(s) => s,
            None => {
                self.mode = Mode::Normal;
                return;
            }
        };
        let n = MENUS.len();
        match key.code {
            KeyCode::Esc | KeyCode::F(10) => {
                self.menu_state = None;
                self.mode = Mode::Normal;
                return;
            }
            KeyCode::Left | KeyCode::BackTab => {
                ms.col = (ms.col + n - 1) % n;
                ms.item = 0;
                ms.open = ms.open && !MENUS[ms.col].items.is_empty();
            }
            KeyCode::Right | KeyCode::Tab => {
                ms.col = (ms.col + 1) % n;
                ms.item = 0;
                ms.open = ms.open && !MENUS[ms.col].items.is_empty();
            }
            KeyCode::Down => {
                if ms.open {
                    let len = MENUS[ms.col].items.len().max(1);
                    ms.item = (ms.item + 1) % len;
                } else if !MENUS[ms.col].items.is_empty() {
                    ms.open = true;
                    ms.item = 0;
                }
            }
            KeyCode::Up => {
                if ms.open {
                    if ms.item == 0 {
                        ms.open = false;
                    } else {
                        ms.item -= 1;
                    }
                }
            }
            KeyCode::Enter => {
                let m = &MENUS[ms.col];
                let action = if !m.items.is_empty() {
                    if ms.open {
                        Some(m.items[ms.item].action)
                    } else {
                        ms.open = true;
                        ms.item = 0;
                        None
                    }
                } else {
                    m.action
                };
                if let Some(a) = action {
                    self.menu_state = None;
                    self.mode = Mode::Normal;
                    self.dispatch(a);
                    return;
                }
            }
            _ => {}
        }
        self.menu_state = Some(ms);
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
    fn ai_generate_reply(&mut self, instruction: String) {
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

        // Only treat the reader's selected message as context when this compose
        // is actually a reply/forward. For a brand-new email, the background
        // message must NOT be pulled in as the "original".
        let is_reply = compose.draft.in_reply_to.is_some();
        let msg = if is_reply { self.current_message().cloned() } else { None };
        let body = if is_reply { self.current_body.clone() } else { None };

        let original = if is_reply {
            body.as_ref()
                .map(crate::ai::body_to_text)
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| crate::ai::unquote(&quoted))
        } else {
            String::new()
        };

        let (from, subject, date) = match &msg {
            Some(m) => (
                m.headers.from.first().map(|a| a.format()).unwrap_or_default(),
                m.headers.subject.clone(),
                m.headers.date.format("%Y-%m-%d %H:%M").to_string(),
            ),
            None => (String::new(), subject_fallback, String::new()),
        };

        if original.trim().is_empty() && user_notes.trim().is_empty() && instruction.trim().is_empty()
        {
            self.set_status(
                "Nothing to work from - type some notes, add an instruction (Ctrl-T), or reply to a message",
            );
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
            instruction,
            is_reply,
        };
        let prompt = crate::ai::build_prompt(&ctx);
        let (tx, rx) = std::sync::mpsc::channel();
        self.ai_rx = Some(rx);
        self.ai_generating = true;
        self.set_status(format!(
            "Generating {} with {}...",
            if is_reply { "reply" } else { "email" },
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
        if o.field == OnboardingField::KeepOnServer {
            o.keep_on_server = !o.keep_on_server;
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

    /// Whether a message has any attachments. Uses the persisted/synced
    /// `has_attachments` flag (set from the Content-Type at envelope time and
    /// corrected once the body is fetched) and the loaded body if present. Does
    /// not trigger a body fetch, so it is cheap to call per list row.
    pub fn message_has_attachments(&self, m: &Message) -> bool {
        m.has_attachments
            || m.body
                .as_ref()
                .map(|b| !b.attachments.is_empty())
                .unwrap_or(false)
    }

    fn open_attachment_viewer(&mut self) {
        // In the conversation view, view the selected thread message's
        // attachments; otherwise the message open in the reader.
        let (attachments, return_mode) = if self.mode == Mode::Thread {
            let body = self.thread_state.as_ref().and_then(|ts| {
                ts.messages
                    .get(ts.selected)
                    .and_then(|m| self.data.message_body(m.id).or_else(|| m.body.clone()))
            });
            match body {
                Some(b) if !b.attachments.is_empty() => (b.attachments, Mode::Thread),
                Some(_) => { self.set_status("No attachments on this message"); return; }
                None => { self.set_status("Message not downloaded yet"); return; }
            }
        } else {
            match self.current_body.as_ref() {
                Some(b) if !b.attachments.is_empty() => (b.attachments.clone(), Mode::Normal),
                Some(_) => { self.set_status("No attachments"); return; }
                None => { self.set_status("Open a message first"); return; }
            }
        };
        self.attachment_viewer = Some(AttachmentViewerState {
            attachments,
            mode: AttachmentViewMode::Listing { selected: 0 },
            save_dest: None,
            return_mode,
        });
        self.mode = Mode::AttachmentViewer;
    }

    fn attachment_view_open(&mut self) {
        let av = match self.attachment_viewer.as_mut() { Some(s) => s, None => return };
        let idx = match &av.mode {
            AttachmentViewMode::Listing { selected } => *selected,
            // Pressing Enter while viewing returns to the listing.
            _ => { av.mode = AttachmentViewMode::Listing { selected: 0 }; return; }
        };
        let att = match av.attachments.get(idx) { Some(a) => a.clone(), None => return };
        let path = match &att.temp_path {
            Some(p) => p.clone(),
            None => {
                self.set_status("Attachment data not available - reopen the message to download it");
                return;
            }
        };
        let kind = crate::attachments::classify(&att.mime_type, &att.filename);
        match kind {
            crate::attachments::AttachmentKind::Image => {
                match crate::attachments::load_image(&path) {
                    Ok(img) => {
                        if let Some(av) = self.attachment_viewer.as_mut() {
                            av.mode = AttachmentViewMode::ViewingImage { idx, image: std::sync::Arc::new(img) };
                        }
                    }
                    Err(e) => self.set_status(format!("Image: {e}")),
                }
            }
            crate::attachments::AttachmentKind::Pdf
            | crate::attachments::AttachmentKind::Docx
            | crate::attachments::AttachmentKind::Text => {
                self.set_status("Extracting...");
                let content = crate::attachments::extract_text(&path, kind)
                    .unwrap_or_else(|e| format!("[{e}]"));
                if let Some(av) = self.attachment_viewer.as_mut() {
                    av.mode = AttachmentViewMode::Viewing { idx, content, scroll: 0 };
                }
                self.clear_status();
            }
            crate::attachments::AttachmentKind::Other => {
                let content = format!(
                    "[Binary file: {}]\nMIME type: {}\nSize: {} bytes\n\nPress [s] to save to Downloads.",
                    att.filename, att.mime_type, att.size
                );
                if let Some(av) = self.attachment_viewer.as_mut() {
                    av.mode = AttachmentViewMode::Viewing { idx, content, scroll: 0 };
                }
            }
        }
    }

    fn attachment_close(&mut self) {
        match self.attachment_viewer.as_ref().map(|av| &av.mode) {
            Some(AttachmentViewMode::Viewing { .. }) | Some(AttachmentViewMode::ViewingImage { .. }) => {
                if let Some(av) = self.attachment_viewer.as_mut() {
                    av.mode = AttachmentViewMode::Listing { selected: 0 };
                }
            }
            _ => {
                let return_mode = self
                    .attachment_viewer
                    .as_ref()
                    .map(|av| av.return_mode)
                    .unwrap_or(Mode::Normal);
                self.attachment_viewer = None;
                self.mode = return_mode;
            }
        }
    }

    fn attachment_save(&mut self) {
        let av = match self.attachment_viewer.as_ref() { Some(s) => s, None => return };
        let idx = match &av.mode {
            AttachmentViewMode::Listing { selected } => *selected,
            AttachmentViewMode::Viewing { idx, .. } => *idx,
            AttachmentViewMode::ViewingImage { idx, .. } => *idx,
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
        form.keep_on_server = acc.keep_on_server;
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
        if self.mode == Mode::Thread {
            if let Some(t) = self.thread_state.as_mut() {
                if t.selected > 0 {
                    t.selected -= 1;
                    t.scroll = 0;
                }
            }
            return;
        }
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
                    AttachmentViewMode::ViewingImage { .. } => {}
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
        if self.mode == Mode::Thread {
            if let Some(t) = self.thread_state.as_mut() {
                if t.selected + 1 < t.messages.len() {
                    t.selected += 1;
                    t.scroll = 0;
                }
            }
            return;
        }
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
                    AttachmentViewMode::ViewingImage { .. } => {}
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
        if self.mode == Mode::Thread {
            if let Some(t) = self.thread_state.as_mut() {
                if delta < 0 {
                    t.scroll = t.scroll.saturating_sub((-delta) as u16);
                } else {
                    t.scroll = t.scroll.saturating_add(delta as u16);
                }
            }
            return;
        }
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
        let state = ComposeState::from_draft(draft, &accounts);
        self.install_compose(state);
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
        let state = ComposeState::from_draft(draft, &accounts);
        self.install_compose(state);
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
        let state = ComposeState::from_draft(draft, &accounts);
        self.install_compose(state);
    }

    fn send_compose(&mut self) {
        let accounts: Vec<Account> = self.accounts.iter().map(|a| a.account.clone()).collect();
        let mut taken = match self.compose.take() {
            Some(c) => c,
            None => return,
        };
        taken.rewrap_body();
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

#[cfg(test)]
mod mouse_tests {
    use super::*;
    use crate::data::InMemoryDataSource;
    use ratatui::layout::Rect;
    use std::sync::Arc;

    fn app() -> App {
        App::new(Arc::new(InMemoryDataSource::sample()))
    }

    #[test]
    fn menu_hit_maps_columns_to_menus() {
        let mut a = app();
        a.ui_menu = Rect { x: 0, y: 0, width: 60, height: 1 };
        // "Account" (with caret) spans cols 0..11.
        assert_eq!(a.menu_hit(0), Some(0));
        assert_eq!(a.menu_hit(10), Some(0));
        // "Settings" begins right after.
        assert_eq!(a.menu_hit(11), Some(1));
        // Far past the last label hits nothing.
        assert_eq!(a.menu_hit(200), None);
    }

    #[test]
    fn sidebar_rows_map_to_account_then_folders() {
        let mut a = app();
        a.ui_sidebar = Rect { x: 0, y: 0, width: 24, height: 20 };
        // Account 0 is expanded by default; row y+1 is its header.
        assert!(matches!(a.sidebar_target_at(1), Some(SidebarTarget::Account(0))));
        assert!(matches!(a.sidebar_target_at(2), Some(SidebarTarget::Folder(0, 0))));
        assert!(matches!(a.sidebar_target_at(6), Some(SidebarTarget::Folder(0, 4))));
        // After account 0's five folders comes account 1's header.
        assert!(matches!(a.sidebar_target_at(7), Some(SidebarTarget::Account(1))));
        // Above the inner area there is nothing.
        assert!(a.sidebar_target_at(0).is_none());
    }

    #[test]
    fn list_rows_map_to_messages() {
        let mut a = app();
        a.settings.show_snippet = false;
        a.ui_list = Rect { x: 0, y: 0, width: 40, height: 20 };
        if a.messages.is_empty() {
            return; // sample seed always has inbox messages, but be defensive
        }
        // First inner row (y+1) is message 0.
        assert_eq!(a.message_at_row(1), Some(0));
        assert_eq!(a.message_at_row(2), Some(1));
        // The top border row is not a message.
        assert_eq!(a.message_at_row(0), None);
    }
}
