-- Add up migration script here
CREATE TABLE confessions(
    id INTEGER PRIMARY KEY,
    message_id INTEGER DEFAULT NULL,
    reviewer_id INTEGER DEFAULT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'Pending',
    rejected_reason TEXT DEFAULT NULL,
    created_at DATETIME NOT NULL DEFAULT (unixepoch())
);
