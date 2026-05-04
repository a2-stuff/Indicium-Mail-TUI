-- Speed up list_by_folder and per-account scans.
CREATE INDEX IF NOT EXISTS idx_messages_folder_date
    ON messages(folder_id, internal_date DESC);

CREATE INDEX IF NOT EXISTS idx_messages_account
    ON messages(account_id);
