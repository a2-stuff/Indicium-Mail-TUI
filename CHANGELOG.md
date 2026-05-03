# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
