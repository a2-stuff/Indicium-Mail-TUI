//! User-configurable runtime settings (auto-refresh interval, mark-as-read,
//! HTML viewer, etc). Mirrored to/from `config.toml` by the binary.

use serde::{Deserialize, Serialize};

/// User-tunable behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Auto-refresh interval in seconds. 0 disables (IDLE still active).
    pub auto_refresh_secs: u32,
    /// Mark a message as `\Seen` when it is opened.
    pub mark_read_on_open: bool,
    /// Render HTML bodies inline via `html2text` (false) or open in `$BROWSER` (true).
    pub html_external: bool,
    /// Browser command for HTML mail. Empty falls back to `xdg-open`.
    pub browser: String,
    /// Show the message preview snippet under each row in the list.
    pub show_snippet: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_refresh_secs: 0,
            mark_read_on_open: true,
            html_external: false,
            browser: String::new(),
            show_snippet: false,
        }
    }
}

/// Field focus inside the Settings modal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    AutoRefreshSecs,
    MarkReadOnOpen,
    HtmlExternal,
    Browser,
    ShowSnippet,
}

impl SettingsField {
    pub fn next(self) -> Self {
        match self {
            Self::AutoRefreshSecs => Self::MarkReadOnOpen,
            Self::MarkReadOnOpen => Self::HtmlExternal,
            Self::HtmlExternal => Self::Browser,
            Self::Browser => Self::ShowSnippet,
            Self::ShowSnippet => Self::AutoRefreshSecs,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::AutoRefreshSecs => Self::ShowSnippet,
            Self::MarkReadOnOpen => Self::AutoRefreshSecs,
            Self::HtmlExternal => Self::MarkReadOnOpen,
            Self::Browser => Self::HtmlExternal,
            Self::ShowSnippet => Self::Browser,
        }
    }
}
