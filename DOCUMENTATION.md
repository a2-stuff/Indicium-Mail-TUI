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
Pure data types: `Account`, `Folder`, `Message`, `Thread`, `Draft`, `Address`, `Flag`, `SyncEvent`, `NewAccountForm`, `OAuthProvider`. No I/O. No async. Every other crate depends on this.

`OAuthProvider` variants: `Google`, `Microsoft { tenant }`, `Yahoo`, `Custom { auth_url, token_url, scope }`.

### `imt-store`
SQLite persistence layer (sqlx + migrations).

- WAL journal mode, foreign keys on, 5s busy timeout
- Tables: `accounts`, `folders`, `messages`, `threads`, `drafts` + FTS5 `messages_fts`
- Repos: `AccountRepo`, `FolderRepo`, `MessageRepo`, `DraftRepo`, `SearchRepo`
- `secrets` module: file storage at `~/.local/share/indicium-mail-tui/secrets/<id>:<kind>` (0600). Set `IMT_USE_KEYRING=1` to route through the OS keyring instead. Keys stored per account: `imap_password`, `smtp_password`, `oauth_access_token`, `oauth_access_expiry`, `oauth_refresh_token`, `oauth_client_secret`.

### `imt-net`
Protocol adapters behind the `MailBackend` async trait.

- `ImapBackend`: connect (implicit TLS / STARTTLS / plain), folder list, fetch envelopes, fetch body, append, set flags, RFC 2177 IDLE push (auto-renewed every 28 minutes; falls back to 30s STATUS poll on servers without IDLE). XOAUTH2 SASL wired in for OAuth2 accounts.
- `SmtpSender`: lettre-based SMTP with the same TLS modes. XOAUTH2 SASL for OAuth2 accounts.
- `oauth`: Authorization Code + PKCE (RFC 7636) + Refresh flow.
  - Providers: Google (`accounts.google.com`), Microsoft 365 (`login.microsoftonline.com/<tenant>`), Yahoo (`api.login.yahoo.com`), Custom (arbitrary endpoints).
  - `OAuthFlow::exchange_code()` - trades authorization code + PKCE verifier for access/refresh tokens via HTTP POST.
  - `OAuthFlow::refresh()` - silently exchanges a refresh token for a new access token.
  - `xoauth2_token()` - builds the base64-encoded XOAUTH2 SASL string for IMAP/SMTP authentication.

### `imt-sync`
Event-driven sync engine.

- `SyncEngine` owns per-account async workers, each:
  1. calls `ensure_fresh_tokens()` to refresh OAuth2 access tokens if within 60 seconds of expiry
  2. connects (emits `AccountConnecting`/`AccountConnected`)
  3. lists folders, persists, emits `FolderListUpdated`
  4. for each folder: select, fetch envelopes for new UIDs, persist, emit `MessageAdded`
  5. enters IDLE on the inbox; on `EXISTS`/`EXPUNGE`/`FETCH` re-syncs and re-enters
- Exponential backoff (5s -> 5min) on connection errors
- `password.rs`: `imap_provider_for(&account)` and `smtp_provider_for(&account)` return auth-method-aware `PasswordProvider` closures (load `imap_password` for password accounts, `oauth_access_token` for OAuth2 accounts); `ensure_fresh_tokens()` handles silent token refresh.
- Public methods: `add_account(account, password, oauth_exchange)`, `remove_account`, `sync_folder`, `fetch_body`, `send`, `shutdown`

`OAuthExchange` (in `engine.rs`): `{ client_id, client_secret, code, verifier, redirect_uri }` - passed through from the TUI onboarding form when saving an OAuth2 account; the engine performs the async HTTP code exchange and stores resulting tokens in secrets.

### `imt-tui`
Ratatui application.

- `App` is the state machine; `run()` owns the terminal lifecycle
- `DataSource` trait is sync (zero-cost reads from a snapshot)
- `Mode` enum: `Normal`, `Compose`, `Help`, `Search`, `Onboarding`, `Settings`, `Accounts`, `Move`, `Info`, `FilePicker`, `AttachmentViewer`, `HtmlViewer`
- Components in `ui/`: `sidebar`, `list`, `reader`, `compose`, `onboarding`, `help`, `search`, `status`, `layout`, `attachment_viewer`
- `App::tick()` runs every 250ms; pulls fresh state from the data source so background sync events become visible automatically

**HTML viewer**: `App::open_html_viewer()` converts the selected HTML part to plain text using `html2text::from_read()` with a 120-character line width and stores it in `app.html_viewer: Option<(String, u16)>`. The `HtmlViewer` mode renders a scrollable modal; scroll offset is the `u16`.

**Attachment viewer**: `is_viewable(mime, filename)` returns true for text/*, common code and data file extensions (`.md`, `.txt`, `.rs`, `.py`, `.js`, `.ts`, `.json`, `.toml`, `.yaml`, `.csv`, `.log`, etc.) and false for binary MIME types regardless of filename. Text attachments are shown inline; binary files display their MIME type, size, and a save-to-disk prompt.

**Onboarding modal**: dynamically adapts its field layout based on the `use_oauth2` toggle on `OnboardingState`. OAuth2 path: Display name, Email, IMAP host/port/TLS, SMTP host/port/TLS, Username, Auth type, Client ID, Client Secret, auth URL display, Auth Code. Password path: same minus the four OAuth2-specific fields. Tabbing to Auth Code triggers `ensure_oauth_url_generated()` which builds a PKCE verifier (32 random bytes, base64url-encoded) and challenge (SHA256 of verifier, base64url-encoded), constructs the provider's authorization URL, and spawns `xdg-open` to open it in the browser.

### `imt`
Binary, integration layer.

- `Snapshot` (RwLock) caches accounts/folders/messages/bodies, hydrated from the DB at startup, kept fresh by a tokio task consuming `SyncEvent`s
- `SyncDataSource` implements `DataSource` against the snapshot for reads and a `Command` channel for writes
- `command_worker` consumes `Command`s and dispatches to `SyncEngine`. `Command::AddAccount` carries an optional `OAuthExchange`; when present, the worker passes it to `engine.add_account()` for async token exchange.
- CLI subcommands handle account management without launching the TUI

## Data flow

### Read path (TUI -> screen)
TUI calls a sync `DataSource` method -> `SyncDataSource` reads from `Snapshot` (RwLock, no I/O) -> returns to UI.

### Write path (TUI -> server)
TUI calls a write method (e.g. `send`) -> `SyncDataSource` posts a `Command` on an unbounded mpsc -> `command_worker` invokes `SyncEngine` -> engine talks to IMAP/SMTP -> emits `SyncEvent` on completion -> snapshot updater task writes back to snapshot -> next `App::tick()` picks it up -> UI re-renders.

### New mail (server -> TUI)
IMAP IDLE delivers `EXISTS` -> account_task ends IDLE, re-syncs the folder, emits `MessageAdded` -> snapshot updater inserts message rows -> next tick the TUI sees a new message in the snapshot and re-renders. No user interaction needed.

### OAuth2 add-account flow
1. User fills the onboarding form with Client ID, tabs to Auth Code
2. TUI generates PKCE verifier + challenge, builds auth URL, calls `xdg-open`
3. User approves in browser, copies `?code=` value from redirect URL
4. User pastes code, presses `Ctrl-S`
5. `OnboardingState::to_form()` produces a `NewAccountForm` with OAuth2 fields populated
6. `SyncDataSource::add_account()` posts `Command::AddAccount { account, password: "", oauth_exchange: Some(...) }`
7. `command_worker` calls `engine.add_account(account, "", Some(exchange))`
8. Engine calls `OAuthFlow::exchange_code()` over HTTPS, stores `oauth_access_token`, `oauth_access_expiry`, `oauth_refresh_token`, `oauth_client_secret` in secrets
9. Account worker starts; before each connection `ensure_fresh_tokens()` auto-refreshes if needed

## Configuration

`~/.config/indicium-mail-tui/config.toml` (loaded if present, ignored otherwise):

```toml
[settings]
auto_refresh_secs = 60          # 0 disables polling; IDLE remains active regardless
mark_read_on_open = true
show_preview_snippet = false
browser = ""                    # override browser for external links
```

Account credentials live in `~/.local/share/indicium-mail-tui/secrets/` (0600 files) or the OS keyring when `IMT_USE_KEYRING=1` is set.

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

- `APPENDUID` is not extracted from IMAP responses (async-imap 0.10 limitation); Sent copies get `uid=0` and rely on the next sync to discover the real UID
- Threading is stubbed (`thread_id: None`); messages are listed flat
- Per-folder index in the sidebar updates on tick; very large mailboxes (>10000 messages) may want pagination beyond the current 500-row cap
- `xdg-open` is used to launch the OAuth2 authorization URL; on macOS use `open` (set via `$BROWSER` or a shell alias)
