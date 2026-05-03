# Indicium Mail TUI

<p align="center">
  <img src="Indicium-Mail-TUI.gif" alt="Indicium Mail TUI demo" />
</p>

A fast, modern terminal email client written in Rust.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.80+-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Ratatui](https://img.shields.io/badge/Ratatui-0.28-blueviolet?logo=data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PHBhdGggZmlsbD0id2hpdGUiIGQ9Ik0yMSAzSDNjLTEuMSAwLTIgLjktMiAydjE0YzAgMS4xLjkgMiAyIDJoMThjMS4xIDAgMi0uOSAyLTJWNWMwLTEuMS0uOS0yLTItMnptMCAxNkgzVjVoMTh2MTR6Ii8+PC9zdmc+)](https://ratatui.rs)
[![Tokio](https://img.shields.io/badge/Tokio-1.40-brightgreen?logo=rust&logoColor=white)](https://tokio.rs)
[![SQLite](https://img.shields.io/badge/SQLite-WAL%20%2B%20FTS5-blue?logo=sqlite&logoColor=white)](https://www.sqlite.org)
[![IMAP](https://img.shields.io/badge/IMAP-RFC%202177%20IDLE-informational?logo=mail.ru&logoColor=white)]()
[![SMTP](https://img.shields.io/badge/SMTP-TLS%20%2F%20STARTTLS-informational?logo=mail.ru&logoColor=white)]()
[![OAuth2](https://img.shields.io/badge/OAuth2-PKCE-success?logo=openid&logoColor=white)]()

## Features

- **Three-pane UI** - sidebar (accounts/folders) + message list + reader
- **IMAP** with real RFC 2177 IDLE push (auto-detects new mail without polling)
- **SMTP** send via STARTTLS or implicit TLS
- **OAuth2** (PKCE) for Gmail, Microsoft 365, Yahoo, and custom providers - full onboarding UI included
- **SQLite** local cache with WAL and FTS5 full-text search
- **Onboarding modal** with provider presets (gmail, outlook, fastmail, yahoo, icloud)
- **Compose / reply / reply-all / forward** with quoted bodies and proper threading headers
- **HTML mail** rendered inline via `html2text` (no browser required)
- **Attachment viewer** - text, code, and markdown files shown inline; binary downloads saved to disk
- **Single static binary** - no runtime dependencies

## Install

Requires Rust 1.80+.

```bash
git clone https://github.com/a2-stuff/Indicium-Mail-TUI.git
cd Indicium-Mail-TUI
cargo install --path crates/imt
```

The `imt` binary lands at `~/.cargo/bin/imt`.

## Quick start

```bash
# Add your first account interactively
imt add-account

# Or non-interactively (password from $IMT_PASSWORD)
IMT_PASSWORD='secret' imt add-account \
  --email you@example.com \
  --imap-host imap.example.com --imap-port 993 --imap-tls implicit \
  --smtp-host smtp.example.com --smtp-port 465 --smtp-tls implicit

# Launch the TUI
imt

# Mock mode (no network, sample inbox)
imt run --mock
```

## CLI

```
imt                              run TUI (default)
imt run [--mock]                 run TUI explicitly
imt add-account [flags]          add account; password from $IMT_PASSWORD or prompt
imt list-accounts                show configured accounts
imt delete-account <uuid>        remove an account
imt --help                       full flag reference
```

Global flags: `--db <path>`, `--config <path>`, `--log-file <path>`.

## Keys (in TUI)

| Key | Action |
|---|---|
| `Tab` / `Shift-Tab` | cycle focus |
| `j` `k` / arrows | move within focused pane |
| `Enter` | open message |
| `Esc` | back |
| `c` | compose |
| `r` / `R` | reply / reply-all |
| `f` | forward |
| `s` | toggle flag |
| `u` | toggle read/unread |
| `m` | account manager |
| `d` | delete (moves to Trash) |
| `v` | move to folder |
| `o` | view HTML body inline (reader pane) |
| `a` | attachment viewer (reader pane) |
| `A` | add account (onboarding modal) |
| `F5` / `Ctrl-R` | refresh / resync current folder |
| `/` | search |
| `i` | info |
| `?` | help overlay |
| `q` | quit |

In compose: `Tab` next field, `Ctrl-A` file picker, `Ctrl-S` send, `Ctrl-D` save draft, `Esc` cancel.

In attachment viewer: `j/k` or arrows scroll, `Enter` or `v` view inline, `s` save to disk, `Esc`/`q` close.

In HTML viewer: `j/k` or arrows scroll, `o`/`Esc`/`q` close.

## OAuth2 setup

When adding an account via the onboarding modal (`A`), switch Auth type to `< OAuth2 >` using the left/right arrow keys. Enter your Client ID (and optionally Client Secret). Tab to the Auth Code field - the browser opens automatically with the authorization URL. Approve access, copy the `?code=...` value from the redirect URL, paste it, then `Ctrl-S` to save.

The app handles token refresh automatically before each IMAP connection.

### Gmail

Enable IMAP in Gmail settings and create an OAuth2 credential in the Google Cloud Console (type: Desktop App). Use `https://mail.google.com/` as the scope.

### Microsoft 365 / Outlook

Register an app in Azure Active Directory with `https://outlook.office365.com/IMAP.AccessAsUser.All` and `https://outlook.office365.com/SMTP.Send` permissions.

### Yahoo Mail

Create an app at the Yahoo Developer Console. Use `mail-w` as the scope.

## Paths

| What | Where |
|---|---|
| Database | `~/.local/share/indicium-mail-tui/imt.sqlite3` |
| Secrets (file fallback) | `~/.local/share/indicium-mail-tui/secrets/` (0600) |
| Config | `~/.config/indicium-mail-tui/config.toml` |
| Logs | `~/.local/share/indicium-mail-tui/imt.log` |

Set `RUST_LOG=imt_sync=debug,imt_net=debug` for protocol-level traces.

## Documentation

See [DOCUMENTATION.md](DOCUMENTATION.md) for architecture and the [CHANGELOG](CHANGELOG.md) for release notes.

## License

MIT - see [LICENSE](LICENSE).
