CREATE TABLE accounts (
    id BLOB PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    address_name TEXT,
    address_email TEXT NOT NULL,
    imap_json TEXT NOT NULL,
    smtp_json TEXT NOT NULL,
    ord INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE folders (
    id BLOB PRIMARY KEY NOT NULL,
    account_id BLOB NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
    path TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT NOT NULL,
    uid_validity INTEGER NOT NULL DEFAULT 0,
    uid_next INTEGER NOT NULL DEFAULT 0,
    message_count INTEGER NOT NULL DEFAULT 0,
    unread_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE(account_id, path)
);

CREATE TABLE messages (
    id BLOB PRIMARY KEY NOT NULL,
    account_id BLOB NOT NULL,
    folder_id BLOB NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
    thread_id BLOB,
    uid INTEGER NOT NULL,
    rfc_message_id TEXT,
    in_reply_to TEXT,
    references_json TEXT NOT NULL DEFAULT '[]',
    from_json TEXT NOT NULL DEFAULT '[]',
    to_json TEXT NOT NULL DEFAULT '[]',
    cc_json TEXT NOT NULL DEFAULT '[]',
    bcc_json TEXT NOT NULL DEFAULT '[]',
    reply_to_json TEXT NOT NULL DEFAULT '[]',
    subject TEXT NOT NULL DEFAULT '',
    date TIMESTAMP NOT NULL,
    internal_date TIMESTAMP NOT NULL,
    flags_json TEXT NOT NULL DEFAULT '[]',
    size INTEGER NOT NULL DEFAULT 0,
    snippet TEXT NOT NULL DEFAULT '',
    body_text TEXT,
    body_html TEXT,
    attachments_json TEXT NOT NULL DEFAULT '[]',
    has_body INTEGER NOT NULL DEFAULT 0,
    UNIQUE(folder_id, uid)
);

CREATE INDEX idx_messages_folder_date ON messages(folder_id, internal_date DESC);
CREATE INDEX idx_messages_thread ON messages(thread_id);
CREATE INDEX idx_messages_rfc ON messages(rfc_message_id);

CREATE TABLE threads (
    id BLOB PRIMARY KEY NOT NULL,
    account_id BLOB NOT NULL,
    folder_id BLOB NOT NULL,
    subject TEXT NOT NULL DEFAULT '',
    last_activity TIMESTAMP NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE drafts (
    id BLOB PRIMARY KEY NOT NULL,
    account_id BLOB NOT NULL,
    kind TEXT NOT NULL,
    in_reply_to BLOB,
    from_json TEXT NOT NULL,
    to_json TEXT NOT NULL DEFAULT '[]',
    cc_json TEXT NOT NULL DEFAULT '[]',
    bcc_json TEXT NOT NULL DEFAULT '[]',
    subject TEXT NOT NULL DEFAULT '',
    body_text TEXT NOT NULL DEFAULT '',
    attachments_json TEXT NOT NULL DEFAULT '[]',
    created_at TIMESTAMP NOT NULL,
    updated_at TIMESTAMP NOT NULL
);
