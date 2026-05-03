//! User-configurable runtime settings. Mirrored to/from `config.toml`.

use serde::{Deserialize, Serialize};

use crate::theme::ThemeName;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub auto_refresh_secs: u32,
    pub mark_read_on_open: bool,
    pub html_external: bool,
    pub browser: String,
    pub show_snippet: bool,
    #[serde(default)]
    pub theme: ThemeName,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_refresh_secs: 60,
            mark_read_on_open: true,
            html_external: false,
            browser: String::new(),
            show_snippet: false,
            theme: ThemeName::Midnight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsField {
    AutoRefreshSecs,
    MarkReadOnOpen,
    HtmlExternal,
    Browser,
    ShowSnippet,
    Theme,
}

impl SettingsField {
    pub fn next(self) -> Self {
        match self {
            Self::AutoRefreshSecs => Self::MarkReadOnOpen,
            Self::MarkReadOnOpen  => Self::HtmlExternal,
            Self::HtmlExternal    => Self::Browser,
            Self::Browser         => Self::ShowSnippet,
            Self::ShowSnippet     => Self::Theme,
            Self::Theme           => Self::AutoRefreshSecs,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::AutoRefreshSecs => Self::Theme,
            Self::MarkReadOnOpen  => Self::AutoRefreshSecs,
            Self::HtmlExternal    => Self::MarkReadOnOpen,
            Self::Browser         => Self::HtmlExternal,
            Self::ShowSnippet     => Self::Browser,
            Self::Theme           => Self::ShowSnippet,
        }
    }
}
