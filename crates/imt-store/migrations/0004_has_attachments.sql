-- Track whether a message carries attachments, so the message list can show
-- an indicator without fetching the full body. Set from the Content-Type at
-- envelope-sync time and corrected to the exact value once the body is fetched.
ALTER TABLE messages ADD COLUMN has_attachments INTEGER NOT NULL DEFAULT 0;
