---
name: indicium-mail
description: Read, search, triage, and reply to email through the Indicium Mail TUI's built-in MCP server (`imt mcp`). Use when the user wants to go through their inbox, read a specific message or thread, draft/send a reply, mark/flag/move messages, or otherwise manage mail with this repo's client - whether or not they keep the TUI open.
---

# Indicium Mail (`imt`)

Indicium Mail TUI is a keyboard-driven terminal email client (IMAP/SMTP, local
SQLite cache). It ships an **MCP server** (`imt mcp`) that exposes the full
read/write lifecycle as tools, so an AI agent can use it as an email client.

Use this skill to operate the user's mailbox through those tools.

## 1. Make sure it's installed and an account exists

```bash
# Build + install the binary (Rust 1.80+). Lands at ~/.cargo/bin/imt
cargo install --path crates/imt --locked

# Configure at least one account (password comes from $IMT_PASSWORD or a prompt)
imt list-accounts                      # check if one is already set up
IMT_PASSWORD='app-password' imt add-account \
  --email you@example.com \
  --imap-host imap.example.com --imap-port 993 --imap-tls implicit \
  --smtp-host smtp.example.com --smtp-port 465 --smtp-tls implicit
```

If `cargo`/`imt` is missing, tell the user to install Rust (`rustup`) and run the
commands above. Provider auto-fill exists for common hosts via `imt add-account`.

## 2. Register the MCP server (one-time, for the agent host)

Claude Code / Claude Desktop config (`claude_desktop_config.json`):

```json
{ "mcpServers": { "indicium-mail": { "command": "imt", "args": ["mcp"] } } }
```

Restart the host; the mail tools appear automatically. (You can also run
`imt mcp` directly and speak JSON-RPC 2.0 over stdin/stdout.)

## 3. Tools (call via MCP)

| Tool | Purpose |
|---|---|
| `list_accounts` | List configured accounts (get `account_id`). |
| `list_folders` | Folders for an account (get `folder_id`, roles, counts). |
| `list_messages` | Messages in a folder (paginated): subject, from, date, snippet, flags. |
| `read_message` | Full body of a message (auto-downloads from IMAP if needed). |
| `search` | Full-text search across cached messages. |
| `send` | Send a new email. Pass `attachments` (a list of local file paths) to attach files. |
| `reply` | Reply / reply-all to a message (correct threading headers). Pass `attachments` (a list of local file paths) to attach files you generated. |
| `mark_read` | Mark read / unread. |
| `toggle_flag` | Star / unstar. |
| `move_message` | Move to another folder. |
| `delete_message` | Move to Trash. |

Typical flow: `list_accounts` -> `list_folders` -> `list_messages` (inbox) ->
`read_message` -> `reply`/`send`. See `MCP_DOCUMENTATION.md` for exact parameter
schemas and examples.

## Operating rules

- **Confirm before sending.** Draft the reply, show it to the user, and only call
  `send`/`reply` after they approve - sending email is irreversible and outward-facing.
- **Never delete** unless the user explicitly asks; `delete_message` only moves to Trash.
- Quote facts only from the actual message/thread; do not invent details.
- For reading a conversation, fetch the message and its related messages (same
  subject / References) so replies in Sent are included.

## Doing it interactively instead

If the user would rather drive it themselves, just run `imt` for the full TUI
(reply `r`, thread view `t`, compose AI-reply `Ctrl-G`, menu `F10`). See `README.md`.
