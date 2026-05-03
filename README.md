# Indicium Mail TUI

A fast, modern terminal email client written in Rust.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

## Features

- **Three-pane UI** - sidebar (accounts/folders) + message list + reader
- **IMAP** with real RFC 2177 IDLE push (auto-detects new mail without polling)
- **SMTP** send via STARTTLS or implicit TLS
- **OAuth2** (PKCE) flow for Google and Microsoft 365
- **SQLite** local cache with WAL and FTS5 full-text search
- **Onboarding modal** with provider presets (gmail, outlook, fastmail, yahoo, icloud)
- **Compose / reply / reply-all / forward** with quoted bodies and proper threading headers
- **HTML mail** rendered inline via `html2text` or opened in `$BROWSER`
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
| `m` | mark read |
| `d` | delete |
| `o` | open HTML body in `$BROWSER` |
| `A` | add account (onboarding) |
| `F5` / `Ctrl-R` | refresh / resync current folder |
| `/` | search |
| `?` | help overlay |
| `q` | quit |

In compose: `Tab` next field, `Ctrl-S` send, `Ctrl-D` save draft, `Esc` cancel.

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
