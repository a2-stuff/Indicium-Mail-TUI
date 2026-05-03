# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.0.13] - 2026-05-03

### Fixed
- **Sidebar unread count went stale when all messages were read**: the old condition `live_unread > 0 || folder.message_count == 0` fell back to the stale server-reported count instead of showing 0. Now checks whether messages have been loaded at all (`!loaded.is_empty()`) and only falls back when the snapshot has no data yet.
- **Read/unread flag not syncing to server**: `Command::SetFlag` was handled inline in the command worker, opening a fresh IMAP connection but never emitting a `MessageFlagsChanged` event, so the DB and snapshot could diverge from the server. Moved to `SyncEngine::set_flag()` which: opens IMAP, stores the flag via `UID STORE`, updates the local DB, then emits `MessageFlagsChanged` - the snapshot reflects the confirmed server state.
- Removed dead `secrets::load` call in the old SetFlag handler (loaded the password but immediately discarded it).

### Added
- **Show preview snippet** setting now works: when enabled in Settings (`Space` on the "Show preview snippet" row), each message row expands to two lines - the subject on top, the snippet dimmed below.

## [0.0.12] - 2026-05-03

### Added
- **Info modal** (`i` in normal mode): shows app name, version, description, GitHub and Twitter links. Closes with `Esc`, `q`, or `i`.

## [0.0.11] - 2026-05-03

### Fixed
- **Ctrl-R refresh was dispatching Reply** instead of refreshing. The `KeyCode::Char('r') if !shift` arm matched first because Ctrl+R has no shift modifier. The ctrl guards now come before the plain letter guards in the match.
- `m` key now opens Account Manager (was `M`/Shift-M). The old `m` mark-read binding is removed; use `u` to toggle read/unread.

### Changed
- Status bar footer reformatted: `[c] compose | [Enter] open | [m] accounts | [,] settings | [/] search | [?] help | [q] quit`
- Help overlay updated: `m` entry corrected to "account manager", `u` and `v` entries added, refresh and settings listed in the App section.

## [0.0.10] - 2026-05-03

### Fixed
- Sidebar unread badge now reflects local read/unread changes immediately by counting unread messages in the cached list, not just the stored `folder.unread_count` (which only updates after a server-side sync).
- Mouse capture no longer enabled; native terminal copy-paste works as expected. The app does not use mouse input anyway.
- `o` (open HTML in browser) detects the absence of a display (no `$DISPLAY` / `$WAYLAND_DISPLAY`) and prints the saved temp file path instead of failing silently. Errors are surfaced to the status bar.

### Changed
- Status bar footer in normal mode trimmed: `Enter open  c compose  Ctrl-R refresh  M accounts  , settings  / search  ? help  q quit`. The full keymap is in the `?` overlay.

## [0.0.9] - 2026-05-03

### Added
- **Auto mark-read after 3 seconds dwell**: opening a message with `Enter` no longer marks it read. Read state is set after the message is in focus in the reader pane for 3 seconds. Disabled when the `mark_read_on_open` setting is off.
- **Mark unread / mark read** key (`u` in normal mode) toggles the `\Seen` flag on the current message.
- **Move to folder** key (`v` in normal mode) opens a folder picker modal; `j/k` to choose, `Enter` to confirm.
- **Delete** (`d`) now actually moves the current message to the account's Trash folder. If no Trash folder exists, the local copy is removed.
- **Reply** (`r`) and **reply-all** (`R`) and **forward** (`f`) keys are documented in the status hint line (already worked).
- **Ctrl-L** added as a refresh keybinding alongside `F5` and `Ctrl-R` (handy on macOS keyboards where `F5` requires `fn`).
- New IMAP method `move_uid` using `UID MOVE` when the server advertises the `MOVE` capability, falling back to `UID COPY` + `STORE +FLAGS \Deleted` + `EXPUNGE`.
- New `SyncEngine::move_message` and `Command::Move` / `Command::Delete` plumbed through the snapshot adapter.

## [0.0.8] - 2026-05-03

### Added
- **Settings modal** (`,` in normal mode): auto-refresh interval (seconds; 0 disables polling, IDLE remains active), mark-as-read on open, HTML external viewer toggle, browser command, show preview snippet. Persisted to `~/.config/indicium-mail-tui/config.toml` on save.
- **Account Manager modal** (`M` in normal mode): list configured accounts with their IMAP/SMTP details. Actions: `Enter`/`e` edit (re-uses the onboarding form pre-filled), `d` delete (with `d`-again confirmation), `a` add. `Esc`/`q` close.
- `DataSource::update_account`, `DataSource::delete_account`, `SyncEngine::update_account`, and `Command::UpdateAccount` / `Command::DeleteAccount` to back the new modal.
- `Settings` info panel explicitly notes that messages are never deleted from the server (fetches use `BODY.PEEK[]`).

### Changed
- Auto-refresh ticks at the configured interval when set; otherwise the IDLE worker handles new mail push.

## [0.0.7] - 2026-05-03

### Fixed
- **Critical**: messages were never persisted on first sync after a folder had pre-existing UIDs. `sync_folder_list` was overwriting `uid_next` with the server's UIDNEXT before `sync_one_folder` ran, so the fetch range computation saw `last_uid_next == server.uid_next` and skipped the envelope fetch entirely. Now the existing `uid_next` and counts are preserved during folder metadata refresh; counts are only updated after a successful envelope fetch.

### Added
- Diagnostic logging at INFO level for envelope fetch range, fetched count, and server UIDNEXT to help future debugging.

### Notes
- Indicium Mail TUI does not, and never has, deleted messages from the server during sync. All IMAP fetches use `BODY.PEEK[]` (which preserves the `\Seen` state) and the codebase has no `EXPUNGE` or `\Deleted` flag setter outside the explicit user-initiated delete action. A Settings panel exposing `leave on server`, auto-refresh interval, mark-as-read-on-open, and other knobs is planned for the next release.

## [0.0.6] - 2026-05-03

### Added
- Refresh action now flips `backend_status` to `refreshing` immediately so the spinner and "Loading..." indicators are visible even when the engine completes the sync in well under one tick.

### Changed
- Folder list ordering: Inbox, custom folders, Archive, Sent, Junk, Trash, **Drafts last**. Applies to both the live backend and the in-memory mock.

## [0.0.5] - 2026-05-03

### Fixed
- Stale `Cancelled` text appearing alongside the `Esc cancel` hint after closing the compose or onboarding modal. The transient status line now decays automatically after ~6 seconds and the cancel handlers no longer post a `Cancelled` message.

## [0.0.4] - 2026-05-03

### Added
- Loading indicator with braille spinner in the status bar and message list pane while the backend is connecting / syncing / refreshing / fetching.
- `(no messages)` placeholder in the message list when a folder is empty (was: blank pane).
- `DataSource::status()` exposes the backend status string to the UI; updated every 250ms via `App::tick`.

## [0.0.3] - 2026-05-03

### Added
- Manual refresh action: `F5` and `Ctrl-R` trigger a resync of the current folder, account, or all accounts depending on focus.
- Auto-detection of new mail in the UI: `App::tick()` now pulls fresh state from the snapshot every 250ms so messages delivered via IDLE appear without user interaction.
- Inbox is now selected by default at startup instead of the first folder in alphabetical order.
- Project documentation: `README.md`, `DOCUMENTATION.md`, `CHANGELOG.md`.
- In-flight body fetch deduplication in `SyncDataSource` to avoid spamming the engine when `App::tick` polls for body availability.

### Changed
- File-based secret storage is now the default; OS keyring is opt-in via `IMT_USE_KEYRING=1`. The previous default broke silently on headless boxes where the keyring backend accepted writes that did not actually persist.

### Fixed
- Folder counts (e.g. unread badge) updating without messages appearing in the list. The cause was that `App::tick` did not pull from the snapshot, so background `SyncEvent`s never reached the UI.

## [0.0.2] - 2026-05-03

### Added
- Real OAuth2 (PKCE) flow for Google and Microsoft 365, with XOAUTH2 SASL wired into both IMAP and SMTP.
- True RFC 2177 IDLE push in the IMAP backend, with 28-minute auto-renew and a polling fallback for servers without IDLE.
- Account onboarding modal (`A` in the sidebar) with provider presets for gmail, outlook, fastmail, yahoo, icloud.
- HTML external viewer mode (`o` opens HTML body in `$BROWSER` when `html_external` is on).
- Snapshot adapter (`SyncDataSource`) bridging the async sync engine to the synchronous TUI data source.
- CLI account management: `add-account`, `list-accounts`, `delete-account`.
- Non-interactive `add-account` with flags + `$IMT_PASSWORD` env var.

### Changed
- Workspace now builds clean end-to-end; release binary is ~8 MB stripped.

## [0.0.1] - 2026-05-03

### Added
- Initial scaffold of the workspace with six crates: `imt-core`, `imt-store`, `imt-net`, `imt-sync`, `imt-tui`, `imt` (binary).
- Domain types: `Account`, `Folder`, `Message`, `Thread`, `Draft`, `Address`, `Flag`, `SyncEvent`.
- SQLite persistence with WAL, foreign keys, FTS5 full-text search, and per-table repositories.
- IMAP (TLS / STARTTLS / plain), SMTP (lettre), MIME building / parsing.
- Per-account async sync engine emitting `SyncEvent`s through an mpsc channel.
- Three-pane Ratatui UI with sidebar / message list / reader, compose modal with To/Cc/Bcc/Subject/body fields, reply / reply-all / forward, search bar, help overlay, status line.
- `imt` binary with TOML config, tracing-subscriber logging, clap arg parsing.
