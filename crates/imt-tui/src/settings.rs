//! User-configurable runtime settings. Mirrored to/from `config.toml`.

use serde::{Deserialize, Serialize};

use crate::theme::ThemeName;

/// AI backend used for the compose-window reply generation (Ctrl-I).
/// Each maps to a locally-installed CLI driven in the background.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    /// Anthropic Claude Code CLI (`claude`).
    Claude,
    /// Google Gemini CLI (`gemini`).
    Gemini,
    /// OpenAI Codex CLI (`codex`).
    Codex,
}

impl Default for AiProvider {
    fn default() -> Self {
        Self::Claude
    }
}

impl AiProvider {
    /// Human-readable label for the settings UI.
    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::Gemini => "Gemini",
            Self::Codex => "Codex",
        }
    }
    /// Next provider in the cycle.
    pub fn next(self) -> Self {
        match self {
            Self::Claude => Self::Gemini,
            Self::Gemini => Self::Codex,
            Self::Codex => Self::Claude,
        }
    }
    /// Previous provider in the cycle.
    pub fn prev(self) -> Self {
        match self {
            Self::Claude => Self::Codex,
            Self::Gemini => Self::Claude,
            Self::Codex => Self::Gemini,
        }
    }
    /// The CLI binary that drives this provider.
    pub fn cli_bin(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Gemini => "gemini",
            Self::Codex => "codex",
        }
    }
}

fn default_ai_model() -> String {
    // "sonnet" is the Claude CLI alias that always resolves to the latest
    // Sonnet model, so the default tracks new releases without edits.
    "sonnet".to_string()
}

fn default_sidebar_frac() -> f32 {
    0.20
}
fn default_list_frac() -> f32 {
    0.35
}

/// Persisted window geometry (compose modal position + size).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct WindowGeom {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

impl WindowGeom {
    pub fn to_rect(self) -> ratatui::layout::Rect {
        ratatui::layout::Rect { x: self.x, y: self.y, width: self.width, height: self.height }
    }
    pub fn from_rect(r: ratatui::layout::Rect) -> Self {
        Self { x: r.x, y: r.y, width: r.width, height: r.height }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub auto_refresh_secs: u32,
    pub mark_read_on_open: bool,
    pub html_external: bool,
    pub browser: String,
    pub show_snippet: bool,
    #[serde(default)]
    pub theme: ThemeName,
    /// AI provider used for compose-window reply generation (Ctrl-I).
    #[serde(default)]
    pub ai_provider: AiProvider,
    /// Model passed to the provider CLI. Empty = the CLI's own default.
    /// For Claude, "sonnet"/"opus"/"haiku" aliases track the latest release.
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    /// Persisted sidebar (accounts) pane width fraction.
    #[serde(default = "default_sidebar_frac")]
    pub sidebar_frac: f32,
    /// Persisted message-list (inbox) pane width fraction.
    #[serde(default = "default_list_frac")]
    pub list_frac: f32,
    /// Persisted compose-window geometry (position + size).
    #[serde(default)]
    pub compose_geom: Option<WindowGeom>,
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
            ai_provider: AiProvider::Claude,
            ai_model: default_ai_model(),
            sidebar_frac: default_sidebar_frac(),
            list_frac: default_list_frac(),
            compose_geom: None,
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
    AiProvider,
    AiModel,
}

impl SettingsField {
    pub fn next(self) -> Self {
        match self {
            Self::AutoRefreshSecs => Self::MarkReadOnOpen,
            Self::MarkReadOnOpen  => Self::HtmlExternal,
            Self::HtmlExternal    => Self::Browser,
            Self::Browser         => Self::ShowSnippet,
            Self::ShowSnippet     => Self::Theme,
            Self::Theme           => Self::AiProvider,
            Self::AiProvider      => Self::AiModel,
            Self::AiModel         => Self::AutoRefreshSecs,
        }
    }
    pub fn prev(self) -> Self {
        match self {
            Self::AutoRefreshSecs => Self::AiModel,
            Self::MarkReadOnOpen  => Self::AutoRefreshSecs,
            Self::HtmlExternal    => Self::MarkReadOnOpen,
            Self::Browser         => Self::HtmlExternal,
            Self::ShowSnippet     => Self::Browser,
            Self::Theme           => Self::ShowSnippet,
            Self::AiProvider      => Self::Theme,
            Self::AiModel         => Self::AiProvider,
        }
    }
}
