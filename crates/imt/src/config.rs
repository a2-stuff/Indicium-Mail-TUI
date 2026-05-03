//! User configuration loaded from `~/.config/indicium-mail-tui/config.toml`.

use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

/// Top-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
}

/// General application settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    /// Open HTML mail in `$BROWSER` instead of rendering to text.
    #[serde(default)]
    pub html_external: bool,
    /// Editor used for compose body when user presses Ctrl-E.
    #[serde(default = "default_editor")]
    pub editor: String,
}

fn default_editor() -> String {
    std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string())
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self { html_external: false, editor: default_editor() }
    }
}

/// Per-account configuration entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub display_name: String,
    pub address: String,
    pub imap_host: String,
    pub imap_port: u16,
    #[serde(default = "default_tls")]
    pub imap_tls: String,
    pub smtp_host: String,
    pub smtp_port: u16,
    #[serde(default = "default_tls")]
    pub smtp_tls: String,
    pub username: String,
}

fn default_tls() -> String {
    "implicit".to_string()
}

impl Config {
    /// Load from `path`, returning the default config if the file does not exist.
    pub fn load_or_default(path: &Path) -> anyhow::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("reading config {}", path.display()))?;
        let cfg: Self = toml::from_str(&text)
            .with_context(|| format!("parsing config {}", path.display()))?;
        Ok(cfg)
    }
}
