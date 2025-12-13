-- Add up migration script here
CREATE TABLE monitor_config (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    host TEXT NOT NULL CHECK (LENGTH(protocol) <= 127),
    port INTEGER NOT NULL,
    protocol TEXT NOT NULL CHECK (LENGTH(protocol) <= 7),
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE UNIQUE INDEX monitor_config_i1 ON monitor_config(host, port, protocol);
