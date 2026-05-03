# Indicium Mail TUI - Documentation

## Architecture

```
+--------------------------------------------------+
|                      imt (bin)                   |
|        clap subcommands, logging, bootstrap      |
+----+--------------------------+------------------+
     |                          |
+----v---------+   +------------v------+
|   imt-tui    |   |  Snapshot adapter |
|  (Ratatui)   |<->|  + command_worker |
+--------------+   +-----+-------------+
                         |
                +--------v---------+
                |    imt-sync      |
                |  per-account     |
                |   workers + IDLE |
                +--+---------+-----+
                   |         |
          +--------v--+   +--v---------+
          | imt-net   |   |  imt-store |
          | IMAP/SMTP |   |  SQLite    |
          | OAuth2    |   |  + FTS5    |
          +-----------+   +------------+

                 imt-core (shared types)
```

## Crates

### `imt-core`
Pure data types: `Account`, `Folder`, `Message`, `Thread`, `Draft`, `Address`, `Flag`, `SyncEvent`, `NewAccountForm`. No I/O. No async. Every other crate depends on this.

### `imt-store`
SQLite persistence layer (sqlx + migrations).

- WAL journal mode, foreign keys on, 5s busy timeout
- Tables: `accounts`, `folders`, `messages`, `threads`, `drafts` + FTS5 `messages_fts`
- Repos: `AccountRepo`, `FolderRepo`, `MessageRepo`, `DraftRepo`, `SearchRepo`
- `secrets` module: file storage at `~/.local/share/indicium-mail-tui/secrets/<id>:<kind>` (0600). Set `IMT_USE_KEYRING=1` to route through the OS keyring instead.

### `imt-net`
Protocol adapters behind the `MailBackend` async trait.

- `ImapBackend`: connect (implicit TLS / STARTTLS / plain), folder list, fetch envelopes, fetch body, append, set flags, RFC 2177 IDLE push (auto-renewed every 28 minutes; falls back to 30s STATUS poll on servers without IDLE)
- `SmtpSender`: lettre-based SMTP with the same TLS modes
- `oauth`: Authorization Code + PKCE + Refresh flow for Google and Microsoft 365; XOAUTH2 SASL helpers wired into both IMAP and SMTP

### `imt-sync`
Event-driven sync engine.

- `SyncEngine` owns per-account async workers, each:
  1. connects (emits `AccountConnecting`/`AccountConnected`)
  2. lists folders, persists, emits `FolderListUpdated`
  3. for each folder: select, fetch envelopes for new UIDs, persist, emit `MessageAdded`
  4. enters IDLE on the inbox; on `EXISTS`/`EXPUNGE`/`FETCH` re-syncs and re-enters
- Exponential backoff (5s -> 5min) on connection errors
- Public methods: `add_account`, `remove_account`, `sync_folder`, `fetch_body`, `send`, `shutdown`

### `imt-tui`
Ratatui application.

- `App` is the state machine; `run()` owns the terminal lifecycle
- `DataSource` trait is sync (zero-cost reads from a snapshot)
- Components in `ui/`: `sidebar`, `list`, `reader`, `compose`, `onboarding`, `help`, `search`, `status`, `layout`
- `App::tick()` runs every 250ms; pulls fresh state from the data source so background sync events become visible automatically

### `imt`
Binary, integration layer.

- `Snapshot` (RwLock) caches accounts/folders/messages/bodies, hydrated from the DB at startup, kept fresh by a tokio task consuming `SyncEvent`s
- `SyncDataSource` implements `DataSource` against the snapshot for reads and a `Command` channel for writes
- `command_worker` consumes `Command`s and dispatches to `SyncEngine`
- CLI subcommands handle account management without launching the TUI

## Data flow

### Read path (TUI -> screen)
TUI calls a sync `DataSource` method -> `SyncDataSource` reads from `Snapshot` (RwLock, no I/O) -> returns to UI.

### Write path (TUI -> server)
TUI calls a write method (e.g. `send`) -> `SyncDataSource` posts a `Command` on an unbounded mpsc -> `command_worker` invokes `SyncEngine` -> engine talks to IMAP/SMTP -> emits `SyncEvent` on completion -> snapshot updater task writes back to snapshot -> next `App::tick()` picks it up -> UI re-renders.

### New mail (server -> TUI)
IMAP IDLE delivers `EXISTS` -> account_task ends IDLE, re-syncs the folder, emits `MessageAdded` -> snapshot updater inserts message rows -> next tick the TUI sees a new message in the snapshot and re-renders. No user interaction needed.

## Configuration

`~/.config/indicium-mail-tui/config.toml` (loaded if present, ignored otherwise):

```toml
[general]
html_external = false           # if true, HTML bodies open in $BROWSER instead of inline
editor = "vi"                   # used by Ctrl-E in compose body (planned)

[[accounts]]
display_name = "Personal"
address = "you@example.com"
imap_host = "imap.example.com"
imap_port = 993
imap_tls = "implicit"           # implicit | starttls | none
smtp_host = "smtp.example.com"
smtp_port = 465
smtp_tls = "implicit"
username = "you@example.com"
```

Account password lives in either the OS keyring (`IMT_USE_KEYRING=1`) or a 0600 file under `~/.local/share/indicium-mail-tui/secrets/`.

## Building

```bash
cargo build --workspace --release
```

Release binary at `target/release/imt` (~8 MB stripped).

## Testing manually

The TUI can be exercised against the in-memory mock with `imt run --mock`. The mock has 2 sample accounts with realistic inbox / sent / drafts / trash / archive folders and 12 sample messages including one HTML.

## Logging

All logs go through `tracing`. Default level is `info`. Override with `RUST_LOG`:

```bash
RUST_LOG=imt_sync=debug,imt_net=trace imt run
```

Logs are written to `~/.local/share/indicium-mail-tui/imt.log` (rotating is the responsibility of the user / journald / logrotate).

## Known limitations

- OAuth2 backend is implemented but the TUI onboarding modal only collects passwords today; OAuth login UI is not yet wired
- `APPENDUID` is not extracted from IMAP responses (async-imap 0.10 limitation); Sent copies get `uid=0` and rely on the next sync to discover the real UID
- Threading is stubbed (`thread_id: None`); messages are listed flat
- Per-folder index in the sidebar updates on tick; very large mailboxes (>10000 messages) may want pagination beyond the current 500-row cap
