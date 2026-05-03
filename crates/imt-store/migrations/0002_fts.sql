CREATE VIRTUAL TABLE messages_fts USING fts5(
    subject,
    from_text,
    to_text,
    body_text,
    content='messages',
    content_rowid='rowid'
);

CREATE TRIGGER messages_ai AFTER INSERT ON messages BEGIN
    INSERT INTO messages_fts(rowid, subject, from_text, to_text, body_text)
    VALUES (new.rowid, new.subject, new.from_json, new.to_json, COALESCE(new.body_text, ''));
END;

CREATE TRIGGER messages_ad AFTER DELETE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, from_text, to_text, body_text)
    VALUES ('delete', old.rowid, old.subject, old.from_json, old.to_json, COALESCE(old.body_text, ''));
END;

CREATE TRIGGER messages_au AFTER UPDATE ON messages BEGIN
    INSERT INTO messages_fts(messages_fts, rowid, subject, from_text, to_text, body_text)
    VALUES ('delete', old.rowid, old.subject, old.from_json, old.to_json, COALESCE(old.body_text, ''));
    INSERT INTO messages_fts(rowid, subject, from_text, to_text, body_text)
    VALUES (new.rowid, new.subject, new.from_json, new.to_json, COALESCE(new.body_text, ''));
END;
