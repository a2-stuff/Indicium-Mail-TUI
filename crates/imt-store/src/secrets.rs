//! Credential storage. Tries the OS keyring first, falls back to a 0600
//! file under `~/.local/share/indicium-mail-tui/secrets/` on headless boxes
//! where no Secret Service / DBus session is available.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use imt_core::AccountId;
use keyring::Entry;
use tracing::{debug, warn};

const SERVICE: &str = "indicium-mail-tui";

fn user(account_id: AccountId, kind: &str) -> String {
    format!("{}:{}", account_id.0, kind)
}

fn fallback_dir() -> Option<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "indicium", "indicium-mail-tui")?;
    let dir = dirs.data_local_dir().join("secrets");
    if let Err(e) = fs::create_dir_all(&dir) {
        warn!(target: "imt-store::secrets", "create secrets dir {}: {}", dir.display(), e);
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    }
    Some(dir)
}

fn fallback_path(account_id: AccountId, kind: &str) -> Option<PathBuf> {
    Some(fallback_dir()?.join(user(account_id, kind)))
}

fn fallback_store(account_id: AccountId, kind: &str, value: &str) -> std::io::Result<()> {
    let Some(path) = fallback_path(account_id, kind) else {
        return Err(std::io::Error::other("no data dir"));
    };
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts.open(&path)?;
    f.write_all(value.as_bytes())?;
    Ok(())
}

fn fallback_load(account_id: AccountId, kind: &str) -> Option<String> {
    let path = fallback_path(account_id, kind)?;
    if !path.exists() {
        return None;
    }
    fs::read_to_string(&path).ok()
}

fn fallback_delete(account_id: AccountId, kind: &str) {
    if let Some(path) = fallback_path(account_id, kind) {
        let _ = fs::remove_file(path);
    }
}

fn use_keyring() -> bool {
    matches!(std::env::var("IMT_USE_KEYRING").as_deref(), Ok("1") | Ok("true"))
}

/// Store a secret value for `(account_id, kind)`.
/// Default: 0600 file under the local data dir. Set `IMT_USE_KEYRING=1` to
/// route to the OS keyring instead.
pub fn store(account_id: AccountId, kind: &str, value: &str) {
    let user = user(account_id, kind);
    if use_keyring() {
        if let Ok(entry) = Entry::new(SERVICE, &user) {
            if let Err(e) = entry.set_password(value) {
                warn!(target: "imt-store::secrets", "keyring set_password failed for {}: {}", user, e);
            } else {
                return;
            }
        }
    }
    if let Err(e) = fallback_store(account_id, kind, value) {
        warn!(target: "imt-store::secrets", "file store failed for {}: {}", user, e);
    } else {
        debug!(target: "imt-store::secrets", "stored secret in file for {}", user);
    }
}

/// Load a secret value. Tries the configured backend first, then the other
/// as a fallback (so a one-time toggle of `IMT_USE_KEYRING` still finds prior
/// secrets).
pub fn load(account_id: AccountId, kind: &str) -> Option<String> {
    let user = user(account_id, kind);
    if use_keyring() {
        if let Ok(entry) = Entry::new(SERVICE, &user) {
            match entry.get_password() {
                Ok(s) => return Some(s),
                Err(keyring::Error::NoEntry) => {}
                Err(e) => debug!(target: "imt-store::secrets", "keyring get failed for {}: {}", user, e),
            }
        }
        return fallback_load(account_id, kind);
    }
    if let Some(v) = fallback_load(account_id, kind) {
        return Some(v);
    }
    if let Ok(entry) = Entry::new(SERVICE, &user) {
        match entry.get_password() {
            Ok(s) => return Some(s),
            Err(keyring::Error::NoEntry) => {}
            Err(e) => debug!(target: "imt-store::secrets", "keyring get failed for {}: {}", user, e),
        }
    }
    None
}

/// Delete a secret from both backends.
pub fn delete(account_id: AccountId, kind: &str) {
    let user = user(account_id, kind);
    if let Ok(entry) = Entry::new(SERVICE, &user) {
        if let Err(e) = entry.delete_credential() {
            debug!(target: "imt-store::secrets", "keyring delete failed for {}: {}", user, e);
        }
    }
    fallback_delete(account_id, kind);
}
