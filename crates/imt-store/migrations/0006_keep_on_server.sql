-- Per-account "leave a copy on the server" preference. Default 1 (keep) so
-- existing accounts never start deleting mail from the server after an upgrade.
ALTER TABLE accounts ADD COLUMN keep_on_server INTEGER NOT NULL DEFAULT 1;
