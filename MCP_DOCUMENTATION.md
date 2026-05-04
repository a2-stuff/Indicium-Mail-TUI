# IMT MCP Server

## Overview

`imt mcp` starts the Indicium Mail TUI as an MCP (Model Context Protocol) server. AI agents (Claude, GPT-4, etc.) can use it as a tool provider to read and manage email programmatically.

The server communicates over **stdin/stdout** using **newline-delimited JSON-RPC 2.0**, following the MCP 2024-11-05 specification. No network port is opened; the agent spawns the process and talks to it directly.

## Setup

### Prerequisites

- `imt` installed and at least one account configured (`imt add-account`)
- An MCP-compatible AI agent (Claude Desktop, any MCP client)

### Claude Desktop configuration

Add to `~/.config/claude/claude_desktop_config.json` (macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "indicium-mail": {
      "command": "imt",
      "args": ["mcp"],
      "env": {}
    }
  }
}
```

Restart Claude Desktop. The mail tools will appear automatically.

### Custom DB path

```json
{
  "mcpServers": {
    "indicium-mail": {
      "command": "imt",
      "args": ["--db", "/path/to/imt.sqlite3", "mcp"]
    }
  }
}
```

### Testing manually

```bash
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"test","version":"1.0"}}}' | imt mcp
```

## Protocol

The server implements MCP 2024-11-05 over JSON-RPC 2.0.

### Handshake

1. Client sends `initialize`
2. Server responds with capabilities (`tools` support)
3. Client sends `initialized` notification
4. Client can now call `tools/list` and `tools/call`

### Session example

```jsonl
→ {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","clientInfo":{"name":"claude","version":"1"}}}
← {"jsonrpc":"2.0","id":1,"result":{"protocolVersion":"2024-11-05","capabilities":{"tools":{}},"serverInfo":{"name":"imt-mcp","version":"0.0.18"}}}
→ {"jsonrpc":"2.0","method":"initialized","params":{}}
→ {"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}
← {"jsonrpc":"2.0","id":2,"result":{"tools":[...]}}
→ {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"list_accounts","arguments":{}}}
← {"jsonrpc":"2.0","id":3,"result":{"content":[{"type":"text","text":"[...]"}],"isError":false}}
```

## Available Tools

### `list_accounts`
List all configured mail accounts.

**Parameters:** none

**Returns:** JSON array of accounts with `id`, `display_name`, `email`, `imap_host`, `imap_port`, `smtp_host`, `smtp_port`, `auth_type`.

---

### `list_folders`
List folders for an account.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `account_id` | string (UUID) | yes | Account ID from `list_accounts` |

**Returns:** JSON array with `id`, `name`, `path`, `role`, `message_count`, `unread_count`.

---

### `list_messages`
List messages in a folder.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `account_id` | string (UUID) | yes | Account ID |
| `folder_id` | string (UUID) | yes | Folder ID from `list_folders` |
| `limit` | integer | no | Max results (default 50) |
| `offset` | integer | no | Pagination offset (default 0) |

**Returns:** JSON array with `id`, `subject`, `from`, `date`, `snippet`, `flags`, `size`, `has_body`.

---

### `read_message`
Fetch the full body of a message. Automatically downloads from IMAP if not yet cached.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `message_id` | string (UUID) | yes | Message ID from `list_messages` |

**Returns:** JSON object with `id`, `subject`, `from`, `to`, `cc`, `date`, `flags`, `body_text`, `body_html`, `snippet`.

**Errors:** if the IMAP body fetch returns no content (server unavailable, message moved, transient failure), the tool returns `body fetch returned no content` rather than empty `body_text`/`body_html`.

---

### `search`
Full-text search across all messages using SQLite FTS5.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `query` | string | yes | Search terms |
| `account_id` | string (UUID) | no | Restrict to one account |
| `limit` | integer | no | Max results (default 25) |

**Returns:** JSON array of matching messages (same shape as `list_messages`).

---

### `send`
Send a new email.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `account_id` | string (UUID) | yes | Sending account |
| `to` | string | yes | Recipient(s), comma-separated |
| `subject` | string | yes | Subject line |
| `body` | string | yes | Plain-text body |
| `cc` | string | no | CC recipients, comma-separated |
| `bcc` | string | no | BCC recipients, comma-separated |

**Returns:** `"Message sent successfully"` or error.

---

### `reply`
Reply to a message.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `message_id` | string (UUID) | yes | Message to reply to |
| `body` | string | yes | Reply body |
| `reply_all` | boolean | no | Reply to all recipients (default false) |

**Returns:** `"Reply sent successfully"` or error.

---

### `mark_read`
Mark a message as read or unread.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `message_id` | string (UUID) | yes | Target message |
| `read` | boolean | no | true = mark read, false = mark unread (default true) |

**Returns:** `"Marked as read"` / `"Marked as unread"` or error.

---

### `toggle_flag`
Toggle the starred/flagged state of a message.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `message_id` | string (UUID) | yes | Target message |

**Returns:** `"Message flagged"` / `"Flag removed"` or error.

---

### `move_message`
Move a message to another folder.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `message_id` | string (UUID) | yes | Message to move |
| `folder_id` | string (UUID) | yes | Destination folder |

**Returns:** `"Message moved"` or error.

---

### `delete_message`
Move a message to Trash. Returns an error if no Trash folder is configured for the account.

**Parameters:**
| Name | Type | Required | Description |
|---|---|---|---|
| `message_id` | string (UUID) | yes | Message to delete |

**Returns:** `"Message moved to Trash"` on success.

**Errors:** `"No Trash folder configured - cannot delete. Move to a folder instead."` when the account has no folder with the Trash role. Use `move_message` to move to a specific folder of your choice.

---

## Example agent workflow

```
Agent: list_accounts
→ [{ "id": "abc-123", "email": "you@gmail.com", ... }]

Agent: list_folders account_id=abc-123
→ [{ "id": "fold-1", "name": "INBOX", "role": "inbox", "unread_count": 3 }, ...]

Agent: list_messages account_id=abc-123 folder_id=fold-1 limit=10
→ [{ "id": "msg-1", "subject": "Hello", "from": "Alice <alice@example.com>", ... }]

Agent: read_message message_id=msg-1
→ { "subject": "Hello", "body_text": "Hi there!\n\nAlice" }

Agent: reply message_id=msg-1 body="Hi Alice, thanks for reaching out!"
→ "Reply sent successfully"
```

## Architecture

```
AI Agent (Claude, etc.)
    │  stdin/stdout JSON-RPC 2.0
    ▼
imt mcp (imt-mcp crate)
    │
    ├── Read tools → SQLite DB (imt-store) — zero network I/O
    │                  AccountRepo / FolderRepo / MessageRepo / SearchRepo
    │
    └── Write tools → SyncEngine (imt-sync) → IMAP / SMTP (imt-net)
                       send, reply, move, set_flag, fetch_body
```

Read operations (list, search) query the local SQLite cache directly - they are fast and work offline. `read_message` will fetch the body from IMAP the first time if it is not cached, then cache it locally.

Write operations go through the SyncEngine which manages IMAP/SMTP connections with TLS, OAuth2 token refresh, and retry logic.

## Credentials and security

Credentials are never sent to the AI agent. The `imt mcp` process loads them from the local secrets store (`~/.local/share/indicium-mail-tui/secrets/`) exactly as the TUI does. The agent only sees message content.

The transport caps each JSON-RPC line at **4 MB** to bound memory against a malformed or hostile client. Larger requests are rejected with an error.

Set `RUST_LOG=imt_mcp=debug` for verbose protocol logging.
