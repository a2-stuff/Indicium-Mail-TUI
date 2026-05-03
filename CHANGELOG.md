# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
