# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.0] - 2026-06-23

### Added
- **AI reply with extra instruction/context** (`Ctrl-T`): opens an "Instruction or Context" dialog where you type an additional instruction (e.g. "keep it short, decline politely"); the reply is generated from the email/thread + your typed notes + that instruction. `Ctrl-T` works in every terminal; `Ctrl-Shift-G` opens the same dialog on terminals with the enhanced keyboard protocol.
- **AI reply can create and attach files.** When you ask the AI reply to generate a file - a text/CSV/Excel/PDF/image/ZIP, etc. - the background model runs in an isolated working directory and writes any final files into an `attachments/` folder; those files are automatically attached to the compose draft when the reply is inserted. The Claude provider runs with the file tools (Read/Write/Edit/Bash) auto-approved so it can write and run a small script to produce real binary files. The status line reports how many files were attached, and the reply body no longer dumps the on-disk path.
- **MCP `send` and `reply` accept an `attachments` parameter.** Any MCP agent can attach files it created by passing a list of local file paths (or `{path, filename}` objects); the MIME type is inferred from the extension and nonexistent paths return a clear error.
- **View attachments from the conversation thread view.** In the thread (`t`) modal, messages with attachments are marked 📎 and the selected message lists its attachment names above the body. Press `a` (or `Enter`) to open the attachment viewer for the selected thread message; closing it returns to the conversation view.
- **Attachment indicator in the message list (all folders, including Sent).** Messages that carry attachments show a 📎 before the subject, so attachment-bearing mail - including emails you sent - is visible at a glance without opening each one. Attachments are detected from the server's **BODYSTRUCTURE** (the exact MIME tree, no attachment bytes downloaded), with a Content-Type header heuristic as a fallback. The flag is persisted (`has_attachments`, migration `0004`) and corrected to the exact value once a body is downloaded. Each folder does a **one-time full rescan** on its next sync so pre-existing messages get the flag (tracked in `folder_attachment_scan`, migration `0005`), and a SQL backfill flags any message whose body was already downloaded.

### Changed
- The AI reply instruction dialog moved from `Ctrl-Shift-G` to `Ctrl-T` so it works on terminals without the enhanced keyboard protocol (`Ctrl-Shift-G` still works where supported).

## [0.2.0] - 2026-06-23

### Added
- **Conversation thread view** (`t`): opens a modal showing every message in the conversation - including replies that live in other folders such as Sent - with the selected message's body below. Threads are reconstructed on demand from `Message-ID` / `In-Reply-To` / `References` (with a normalized-subject fallback), since `thread_id` is not populated by the sync engine. The reader shows a "Thread: N messages - press [t] to view" hint when the current message has a conversation. New `DataSource::thread()` gathers messages across all loaded folders.
- **Shareable Claude Code skill** (`.claude/skills/indicium-mail`): lets anyone who clones the repo use the mailbox through the `imt mcp` tools (read / search / reply / send / triage), with setup and operating rules.

## [0.1.1] - 2026-06-23

### Fixed
- **Compose body now wraps as you type.** Long lines break at word boundaries live while editing (the caret stays where you are), not only on resize/send.
- **Manual line breaks are preserved.** Wrapping is now break-only and no longer merges your own line breaks, so a multi-line signature stays multi-line when sent (previously it collapsed to one line).
- **AI reply leaves the caret at the top** of the body instead of jumping to the bottom (no more scrolling up to read the generated reply).

## [0.1.0] - 2026-06-23

### Added
- **AI reply generation in compose (`Ctrl-G`).** Drafts or refines a reply in the background via a local AI CLI and inserts it at the body cursor. An empty body generates a reply from the email/thread; if you have typed notes, they are expanded and polished into the reply (and the typed notes are replaced, not duplicated). Output is normalized to natural paragraphs (single blank line between paragraphs) instead of robotic per-line spacing.
  - **Provider selector** in Settings: **Claude**, **Gemini**, or **Codex** (each drives its CLI; a missing CLI reports a clear message).
  - **Model** field in Settings (empty = the CLI's default). Default is Claude with the `sonnet` alias, which always tracks the latest Sonnet.
  - Runs off the UI thread; Claude is invoked with `--strict-mcp-config` and a neutral working directory, and the process is stopped once the reply is read so slow exit hooks don't block insertion.
  - Note: `Ctrl-I` is not usable for this because terminals send it as `Tab`.
- **Top menu bar** (`F10`) with interactive dropdowns: `Account` (Add Account / Manage Accounts / Refresh), `Settings`, `Info`, `Help`, `Quit`. Navigate with arrows/Tab, Enter to run, Esc to exit. Entries reuse the existing keyboard actions.
- **Attachment viewer** in a centered floating modal (works over SSH):
  - **Images** rendered visually as truecolor half-block cells (no graphics protocol required; aspect-preserving).
  - **PDF** text extracted and shown scrollable; **Word (.docx)** text extracted from the document body.
  - The attachment list shows each item's type and whether it is viewable.
- **Draggable + resizable compose window** via mouse: drag the title bar to move, drag the bottom-right corner to resize. The body **hard-wraps** to the window width (per paragraph; quoted `>` lines kept intact) on resize, after AI insert, and on send.
- **Resizable panes**: drag the vertical dividers between the accounts, inbox, and reading panes to resize them.
- **Persistent layout**: pane widths and the compose window's size/position are saved to `config.toml` and restored on the next launch.
- **Mouse support** is now enabled (Shift+drag still selects text in most terminals).

### Changed
- Footer/status line reorganized into grouped shortcut sections with a dedicated MENU mode hint.

## [0.0.19] - 2026-05-14

### Fixed
- **Sidebar unread/total counts stayed stale after a move or delete in folders the user had not opened.** The sync engine now recomputes message and unread counts for both the source and destination folder after every `move_message`, persists them via `FolderRepo::update_counts`, and emits `FolderCountsChanged` for each. The sidebar updates everywhere immediately, not just for the currently loaded folder.

### Added
- **Empty Trash**: press `Shift+E` while viewing the Trash folder to permanently delete every message in it. Refuses to run outside Trash.
- New `MailBackend::expunge_folder` trait method and IMAP implementation (`UID STORE 1:* +FLAGS \Deleted` followed by `EXPUNGE`).
- `SyncEngine::empty_trash(folder_id)` plumbed through `Command::EmptyTrash` and a new `DataSource::empty_trash` hook. Optimistically clears the snapshot before the IMAP round-trip.
- `MessageRepo::delete_by_folder` for the local-side cleanup.

## [0.0.18] - 2026-05-04

### Fixed
- **Optimistic snapshot updates** are now sent to the engine before mutating local state - prevents UI showing a successful move/delete when the engine is shut down.
- **Toggle flag race**: state is captured before mutation, so concurrent updates can no longer flip the wrong way.
- **Move with DB delete failure** now propagates the error and triggers a resync instead of leaving an orphan envelope row.
- **Sent message duplicate**: dropped the local stub envelope; rely on next folder sync to fetch the real envelope. Eliminates the "two copies in Sent" bug after a transient DB error.
- **OAuth2 expiry edge cases**: missing or malformed `oauth_access_expiry` now forces a refresh rather than silently skipping. Missing refresh token returns a clear "please re-authenticate" error.
- **Delete with no Trash folder** now returns an error instead of silently dropping the message locally while leaving it on the server.
- **MCP `read_message`** returns an explicit error if the body fetch returns no content, instead of returning empty `body_text`/`body_html`.

### Added
- **Body LRU cache**: snapshot's body cache is now bounded to 1000 entries, preventing unbounded memory growth on long-running sessions.
- **TLS=None warnings** are emitted at IMAP/SMTP connect when plaintext mode is configured.
- **OAuth2 `state` parameter** added to the auth URL to defend against CSRF; verified on redirect.
- **MCP transport input size cap** (4 MB per JSON-RPC line) prevents a malformed/malicious client from exhausting memory.
- **Pre-sorted snapshot messages**: messages are inserted in `internal_date DESC` order; eliminates per-render sort cost in the TUI.

### Changed
- **DB index** `idx_messages_folder_date` (`folder_id, internal_date DESC`) and `idx_messages_account` for faster `list_by_folder` and per-account scans.
- **Account workers** spawn in parallel at startup via `join_all` instead of sequentially - cuts startup time on multi-account setups.
- **Release build profile**: `lto = "fat"`, `panic = "abort"` for ~15-20% smaller binary.
- **Tokio features trimmed** from `"full"` to the explicit subset the codebase uses.
- **CLI**: `--imap-tls` and `--smtp-tls` reject invalid values at parse time via clap `value_parser`.
- **OAuth2 PKCE consolidation**: TUI no longer duplicates verifier/challenge generation - delegates to `imt_net::OAuthFlow`.

## [0.0.17] - 2026-05-04

### Added
- **MCP server** (`imt mcp`): start Indicium Mail TUI as a Model Context Protocol server so AI agents can manage email via tool calls. Communicates over stdin/stdout with newline-delimited JSON-RPC 2.0, implementing the MCP 2024-11-05 specification.
- **12 MCP tools**: `list_accounts`, `list_folders`, `list_messages`, `read_message`, `search`, `send`, `reply`, `mark_read`, `toggle_flag`, `move_message`, `delete_message` - covering the full read/write email lifecycle.
- Read tools query the local SQLite cache directly (no network); `read_message` auto-fetches body from IMAP on first access. Write tools go through the existing SyncEngine with OAuth2 token refresh, TLS, and retry logic.
- `MCP_DOCUMENTATION.md`: complete guide covering Claude Desktop setup, JSON-RPC protocol flow, all tool schemas with parameter tables, an example agent workflow, and architecture diagram.

## [0.0.16] - 2026-05-03

### Added
- **Inline HTML viewer**: pressing `o` in the reader pane renders the HTML body using `html2text` and displays it in a scrollable TUI modal (`j/k` or arrows to scroll, `o`/`Esc`/`q` to close). No browser is opened or required.
- **Full OAuth2 onboarding UI**: the Add Account modal now includes an Auth type toggle (`< Password >` / `< OAuth2 >`). Selecting OAuth2 reveals Client ID, Client Secret (optional), and Auth Code fields. Tabbing to Auth Code automatically generates a PKCE verifier + challenge and opens the provider's authorization URL via `xdg-open`. Paste the `?code=` value from the redirect URL and `Ctrl-S` to save. Token exchange and storage happen in the background.
- **Yahoo Mail OAuth2**: full support alongside Google and Microsoft 365, using `https://api.login.yahoo.com/oauth2/` endpoints and the `mail-w` scope.
- **Custom OAuth2 provider**: specify arbitrary `auth_url`, `token_url`, and `scope` values for any compliant OAuth2 provider.
- **Automatic token refresh**: before each IMAP connection, `ensure_fresh_tokens()` checks the stored expiry timestamp and performs a silent refresh (using the stored refresh token) if the access token is within 60 seconds of expiring.
- **Attachment content viewer**: text, code, and markdown files (`.md`, `.txt`, `.rs`, `.py`, `.json`, `.csv`, `.log`, etc.) are now shown inline in the attachment viewer rather than being labelled "Binary file". Viewer is opened with `Enter` or `v`; binary files still show MIME type and size.
- **"No attachments" toast**: pressing `a` in the reader pane on a message with no attachments now shows a transient toast notification instead of silently doing nothing.

### Changed
- `o` key behaviour changed: was "open HTML in `$BROWSER`" (external), is now "open inline HTML viewer" (TUI modal). The `html_external` config key is retained for backwards compatibility but no longer has effect.
- `a` key (attachment viewer) is now only active when focus is on the reader pane; it has no effect in the message list or sidebar.

## [0.0.15] - 2026-05-03

### Added
- **File browser for attachments**: pressing `Ctrl-A` in the compose modal (when on any field) opens a full file picker. Navigate with `j/k`, `Enter` to toggle selection or enter a directory, `Backspace` to go up, `a` to confirm all selected files, `Esc` to cancel. Multiple files can be selected simultaneously; each shows `[x]` when picked. Directories show bold with `[d]`. File sizes are displayed.
- **Remove attachments**: with the Attachments field focused in compose, press `Backspace`/`Delete` to remove the last attachment one at a time.
- 25+ MIME type mappings (PDF, images, Office, archives, audio/video, plain text, JSON/XML, etc.).

### Changed
- Attachment row in compose now shows count and filenames when attachments are present, with inline remove hint.

## [0.0.14] - 2026-05-03

### Added
- **Toast notifications**: transient status messages (account added, moved, refreshing, errors, etc.) now appear as a floating rounded-border box in the bottom-right corner instead of being squeezed into the status bar. Error messages render in red; informational ones in the accent colour. The toast auto-clears after ~6 seconds as before.

### Changed
- Status bar: removed `|` separators between hotkey hints - now separated by two spaces.
- Status bar: removed the inline status text (replaced by the toast overlay).
- Sync indicator in the status bar simplified: `[ready]` / `[⠙ syncing]` - brackets dropped, just the text.
- **Auto-refresh default changed from 0 (off) to 60 seconds.** New mail is checked every 60 seconds across all folders in addition to the IMAP IDLE push on the inbox. Set to 0 in Settings to disable polling and rely on IDLE only.

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
