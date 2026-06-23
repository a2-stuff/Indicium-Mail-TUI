-- Records which folders have had a one-time full attachment scan (BODYSTRUCTURE
-- re-fetch of every message). Presence of a row with scanned=1 means done, so
-- the sync only pays the full-folder rescan once per folder. Kept in a side
-- table to avoid widening the folders row.
CREATE TABLE IF NOT EXISTS folder_attachment_scan (
    folder_id BLOB PRIMARY KEY,
    scanned   INTEGER NOT NULL DEFAULT 0
);

-- Immediate backfill: any message whose full body is already downloaded and
-- has at least one attachment part gets its flag set now, without waiting for a
-- rescan.
UPDATE messages
   SET has_attachments = 1
 WHERE has_body = 1
   AND attachments_json IS NOT NULL
   AND attachments_json <> '[]'
   AND attachments_json <> '';
