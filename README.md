# Indicium Mail TUI

<p align="center">
  <img src="Indicium-Mail-TUI.gif" alt="Indicium Mail TUI demo" />
</p>

<table align="center">
  <tr>
    <td><img src="screen1.png" alt="Screenshot 1" width="100%" /></td>
    <td><img src="screen2.png" alt="Screenshot 2" width="100%" /></td>
  </tr>
  <tr>
    <td><img src="screen3.png" alt="Screenshot 3" width="100%" /></td>
    <td><img src="screen4.png" alt="Screenshot 4" width="100%" /></td>
  </tr>
</table>

A keyboard-driven terminal email client built in Rust. Reads and sends mail over standard IMAP/SMTP with real push delivery via RFC 2177 IDLE, stores everything locally in SQLite, and never opens a browser - HTML renders inline, OAuth2 flows run inside the TUI itself.

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.80+-orange?logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Ratatui](https://img.shields.io/badge/Ratatui-0.28-blueviolet?logo=data:image/svg+xml;base64,PHN2ZyB4bWxucz0iaHR0cDovL3d3dy53My5vcmcvMjAwMC9zdmciIHZpZXdCb3g9IjAgMCAyNCAyNCI+PHBhdGggZmlsbD0id2hpdGUiIGQ9Ik0yMSAzSDNjLTEuMSAwLTIgLjktMiAydjE0YzAgMS4xLjkgMiAyIDJoMThjMS4xIDAgMi0uOSAyLTJWNWMwLTEuMS0uOS0yLTItMnptMCAxNkgzVjVoMTh2MTR6Ii8+PC9zdmc+)](https://ratatui.rs)
[![Tokio](https://img.shields.io/badge/Tokio-1.40-brightgreen?logo=rust&logoColor=white)](https://tokio.rs)
[![SQLite](https://img.shields.io/badge/SQLite-WAL%20%2B%20FTS5-blue?logo=sqlite&logoColor=white)](https://www.sqlite.org)
[![IMAP](https://img.shields.io/badge/IMAP-RFC%202177%20IDLE-informational?logo=mail.ru&logoColor=white)]()
[![SMTP](https://img.shields.io/badge/SMTP-TLS%20%2F%20STARTTLS-informational?logo=mail.ru&logoColor=white)]()
[![OAuth2](https://img.shields.io/badge/OAuth2-PKCE-success?logo=openid&logoColor=white)]()
[![MCP](https://img.shields.io/badge/MCP-2024--11--05-blueviolet?logo=anthropic&logoColor=white)](MCP_DOCUMENTATION.md)
[![Linux](https://img.shields.io/badge/Linux-supported-FCC624?logo=linux&logoColor=black)]()
[![macOS](https://img.shields.io/badge/macOS-supported-000000?logo=apple&logoColor=white)]()
[![Windows](https://img.shields.io/badge/Windows-supported-0078D6?logo=windows&logoColor=white)]()

## Features

**Mail**
- Real RFC 2177 IDLE push - new mail appears instantly without polling
- IMAP implicit TLS, STARTTLS, and plain; SMTP with the same three modes
- Compose, reply, reply-all, and forward with quoted bodies and correct threading headers (`In-Reply-To`, `References`)
- Full-text search across all cached messages via SQLite FTS5
- Move to folder, delete to Trash, flag, mark read/unread
- AI reply drafting in compose (`Ctrl-G`) via a local Claude / Gemini / Codex CLI - generates from the thread, or expands the notes you typed

**Reading**
- Three-pane layout: account/folder sidebar, message list, reader
- Conversation thread view (`t`): see the whole conversation - including replies in other folders (e.g. Sent) - grouped by Message-ID / References, with a subject fallback
- HTML bodies rendered inline via `html2text` - no browser needed
- Attachment viewer (centered modal): images rendered inline as truecolor half-blocks (works over SSH, no graphics protocol needed), PDF and Word (.docx) shown as extracted text, text/code/markdown inline; other binaries identified by MIME type and saved to disk
- Resizable, draggable UI: drag pane dividers to resize the account/inbox/reading panes; drag the compose window to move/resize it - all remembered across restarts
- Auto mark-read after a configurable dwell time (default 3 seconds)
- Preview snippets in the message list (optional)

**Account setup**
- Interactive onboarding modal with provider auto-fill for Gmail, Outlook, Fastmail, Yahoo, and iCloud
- Password auth or full OAuth2 (PKCE) - switch auth type inside the modal with `←/→`
- OAuth2 for Gmail, Microsoft 365, Yahoo Mail, and any custom provider; auth URL opens automatically, token exchange and refresh happen in the background
- Multi-account support with an account manager (`m`)

**AI agent integration**
- MCP server (`imt mcp`) - expose your inbox as tools to any MCP-compatible AI agent
- 12 tools covering the full read/write lifecycle: list, search, read, send, reply, move, flag, delete
- One-line Claude Desktop config; works with any MCP client

**Other**
- SQLite local cache with WAL mode - fast reads, no corruption on crash
- Configurable auto-refresh interval; IDLE push always active
- Toast notifications for async events (sync errors, account added, mail moved)
- File picker for attaching files in compose (`Ctrl-A`)
- Theming support
- Single static binary, no runtime dependencies

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
| `t` | view conversation thread |
| `s` | toggle flag |
| `u` | toggle read/unread |
| `m` | account manager |
| `F10` | open the menu bar (arrows/Tab to move, Enter to run, Esc to exit) |
| `d` | delete (moves to Trash) |
| `E` | empty Trash (only in Trash folder) |
| `v` | move to folder |
| `o` | view HTML body inline (reader pane) |
| `a` | attachment viewer (reader pane) |
| `A` | add account (onboarding modal) |
| `F5` / `Ctrl-R` | refresh / resync current folder |
| `/` | search |
| `i` | info |
| `?` | help overlay |
| `q` | quit |

In compose: `Tab` next field, `Ctrl-G` AI reply, `Ctrl-T` AI reply with an instruction/context prompt, `Ctrl-A` file picker, `Ctrl-S` send, `Ctrl-D` save draft, `Esc` cancel. Drag the title bar to move the window and the bottom-right corner to resize it; the body word-wraps to the window width.

## Mouse

Mouse support is enabled. Drag the dividers between the account, inbox, and reading panes to resize them; drag the compose window's title bar to move it and its bottom-right corner to resize it. Pane sizes and the compose window's position/size persist across restarts. In most terminals, hold `Shift` while dragging to do native text selection / copy-paste.

## AI reply

In the compose window, press `Ctrl-G` to draft a reply with a local AI CLI:

- With an empty body it generates a reply from the email/thread.
- If you have typed a few notes (e.g. a time and place), it expands and polishes them into a full reply using the thread context - your notes are woven in, not duplicated.

Pick the provider and model in Settings (`,` → AI reply provider / AI model):

- **Claude** (`claude`), **Gemini** (`gemini`), or **Codex** (`codex`). The chosen provider's CLI must be installed and on `PATH`.
- Model is provider-specific; leave it empty for the CLI's default. The default is Claude with the `sonnet` alias, which always tracks the latest Sonnet.

Generation runs in the background and is inserted at the cursor when ready.

Press `Ctrl-T` instead to open an **Instruction or Context** dialog: type an
extra instruction (e.g. "keep it short and decline politely") and press Enter -
the reply uses the email/thread plus your typed notes plus that instruction.
(`Ctrl-Shift-G` does the same on terminals with the enhanced keyboard protocol -
kitty, foot, WezTerm, ghostty, recent xterm - but `Ctrl-T` works everywhere.)

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

## MCP - AI agent integration

`imt mcp` starts the MCP server on stdin/stdout so any MCP-compatible AI agent can read and manage your email via tool calls.

### Claude Desktop

Add to `claude_desktop_config.json` (macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "indicium-mail": {
      "command": "imt",
      "args": ["mcp"]
    }
  }
}
```

Restart Claude Desktop - the mail tools appear automatically.

### Available tools

| Tool | What it does |
|---|---|
| `list_accounts` | List configured accounts |
| `list_folders` | List folders for an account |
| `list_messages` | List messages in a folder (paginated) |
| `read_message` | Fetch full body, auto-downloads from IMAP if not cached |
| `search` | Full-text search via SQLite FTS5 |
| `send` | Send a new email |
| `reply` | Reply or reply-all to a message |
| `mark_read` | Mark read or unread |
| `toggle_flag` | Star / unstar a message |
| `move_message` | Move to another folder |
| `delete_message` | Move to Trash |

See [MCP_DOCUMENTATION.md](MCP_DOCUMENTATION.md) for the full protocol reference, parameter schemas, and example agent workflows.

## Documentation

See [DOCUMENTATION.md](DOCUMENTATION.md) for architecture and the [CHANGELOG](CHANGELOG.md) for release notes.

## License

MIT - see [LICENSE](LICENSE).
