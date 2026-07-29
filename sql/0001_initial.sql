-- =============================================================
-- Noye: server health-monitoring system, initial schema
-- =============================================================

-- Users table (RBAC: admin / member)
CREATE TABLE IF NOT EXISTS users (
    id          TEXT PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    is_active   INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_users_email ON users(email);

-- Targets table (requirement 2-2)
CREATE TABLE IF NOT EXISTS targets (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    type             TEXT NOT NULL CHECK (type IN ('http', 'https', 'tcp', 'smtp', 'tls')),
    host             TEXT NOT NULL,
    port             INTEGER,
    path             TEXT DEFAULT '/',
    expected_status  INTEGER DEFAULT 200,
    body_contains    TEXT,                        -- Body substring required for success
    tls_threshold_days INTEGER DEFAULT 30,        -- Minimum acceptable days until certificate expiry
    timeout_sec      INTEGER NOT NULL DEFAULT 10,
    retry_count      INTEGER NOT NULL DEFAULT 3,
    interval_minutes INTEGER NOT NULL DEFAULT 5,
    is_disabled      INTEGER NOT NULL DEFAULT 0,
    owner_id         TEXT NOT NULL,
    tags             TEXT,                        -- Tags as a JSON array
    next_check_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    created_by       TEXT NOT NULL,
    updated_by       TEXT NOT NULL,
    FOREIGN KEY(owner_id) REFERENCES users(id)
);
CREATE INDEX idx_targets_next_check ON targets(next_check_at) WHERE is_disabled = 0;
CREATE INDEX idx_targets_owner ON targets(owner_id);
CREATE INDEX idx_targets_type ON targets(type);

-- State table (requirement 2-4: tracks consecutive successes / failures)
CREATE TABLE IF NOT EXISTS target_states (
    target_id               TEXT PRIMARY KEY,
    current_status          TEXT NOT NULL DEFAULT 'unknown'
                            CHECK (current_status IN ('up', 'down', 'degraded', 'maintenance', 'unknown')),
    consecutive_successes   INTEGER NOT NULL DEFAULT 0,
    consecutive_failures    INTEGER NOT NULL DEFAULT 0,
    success_threshold       INTEGER NOT NULL DEFAULT 3,  -- Threshold for recovery decision
    failure_threshold       INTEGER NOT NULL DEFAULT 3,  -- Threshold for outage decision
    last_checked_at         TEXT,
    last_status_change_at   TEXT,
    last_notification_at    TEXT,                         -- Used to suppress duplicate notifications
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);

-- Check results (time-series data)
CREATE TABLE IF NOT EXISTS check_results (
    id              TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL,
    checked_at      TEXT NOT NULL DEFAULT (datetime('now')),
    is_success      INTEGER NOT NULL,
    status_code     INTEGER,
    response_time_ms INTEGER,
    error_message   TEXT,
    tls_expiry_date TEXT,                        -- Expiry date of the TLS certificate
    tls_days_left   INTEGER,
    details         TEXT,                         -- Additional details encoded as JSON
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);
CREATE INDEX idx_results_target_time ON check_results(target_id, checked_at DESC);
CREATE INDEX idx_results_checked_at ON check_results(checked_at);

-- Incidents (records each state transition)
CREATE TABLE IF NOT EXISTS incidents (
    id              TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'acknowledged')),
    opened_at       TEXT NOT NULL DEFAULT (datetime('now')),
    resolved_at     TEXT,
    duration_sec    INTEGER,
    cause           TEXT,
    resolution_note TEXT,
    created_by      TEXT,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);
CREATE INDEX idx_incidents_target ON incidents(target_id, opened_at DESC);
CREATE INDEX idx_incidents_status ON incidents(status);

-- Maintenance windows (requirement 2-4: notification suppression)
CREATE TABLE IF NOT EXISTS maintenance_windows (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    start_at        TEXT NOT NULL,
    end_at          TEXT NOT NULL,
    target_tag      TEXT,                         -- Match by tag (NULL means all targets)
    target_id       TEXT,                         -- Match a single target
    suppress_notify INTEGER NOT NULL DEFAULT 1,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    created_by      TEXT NOT NULL,
    updated_by      TEXT NOT NULL,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE SET NULL
);
CREATE INDEX idx_maint_active ON maintenance_windows(start_at, end_at) WHERE is_active = 1;

-- Notification channel configuration
CREATE TABLE IF NOT EXISTS notification_channels (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    channel_type    TEXT NOT NULL CHECK (channel_type IN ('webhook', 'email', 'slack')),
    endpoint        TEXT NOT NULL,                -- URL or email address
    is_enabled      INTEGER NOT NULL DEFAULT 1,
    owner_id        TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    FOREIGN KEY(owner_id) REFERENCES users(id)
);

-- Many-to-many between targets and notification channels
CREATE TABLE IF NOT EXISTS target_notifications (
    target_id   TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    on_down     INTEGER NOT NULL DEFAULT 1,
    on_up       INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(target_id, channel_id),
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE,
    FOREIGN KEY(channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE
);

-- Audit log (requirement 4-1)
CREATE TABLE IF NOT EXISTS audit_logs (
    id              TEXT PRIMARY KEY,
    action_time     TEXT NOT NULL DEFAULT (datetime('now')),
    actor_id        TEXT NOT NULL,
    actor_email     TEXT,
    resource_type   TEXT NOT NULL,                -- target, user, maintenance, notification, etc.
    resource_id     TEXT,
    action_type     TEXT NOT NULL,                -- create, update, delete, login, manual_resolve, etc.
    previous_value  TEXT,                          -- Previous value, encoded as JSON
    new_value       TEXT,                          -- New value, encoded as JSON
    result          TEXT NOT NULL DEFAULT 'success' CHECK (result IN ('success', 'failure')),
    ip_address      TEXT,
    -- Hash-chain columns (since 0.27.2). See crates/core/src/db/audit/hash.rs
    -- and docs/security-posture.md#audit-logging.
    --
    -- Each row carries:
    --   prev_hash  - the row_hash of the immediately prior row (or 64 hex
    --                zeros for the genesis row), enabling chain traversal.
    --   row_hash   - SHA-256 over (prev_hash || canonical_serialization(row)),
    --                pinning the row's content to its position in the chain.
    --
    -- A database provisioned before this file added these columns (tag
    -- 0.1.0 — "Class A" in requirements.md G-01) has neither. Its rows are
    -- reconciled by migration 0004, which gives them NULL values; the
    -- verification routine in crates/core/src/db/audit.rs treats NULL rows
    -- as "legacy rows" and skips them — the chain begins fresh with the
    -- next INSERT. See docs/src/decision-log.md DEC-010 for why this file
    -- was amended in place rather than adding these columns via a
    -- migration `0002` (which is retired and must not be reused).
    prev_hash       TEXT,                          -- row_hash of the prior row, or 64 hex zeros at genesis
    row_hash        TEXT,                          -- SHA-256(prev_hash || canonical_serialization(row))
    FOREIGN KEY(actor_id) REFERENCES users(id)
);
CREATE INDEX idx_audit_time ON audit_logs(action_time DESC);
CREATE INDEX idx_audit_actor ON audit_logs(actor_id);
CREATE INDEX idx_audit_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_row_hash ON audit_logs(row_hash);

-- Data lifecycle management metadata
CREATE TABLE IF NOT EXISTS retention_policies (
    table_name      TEXT PRIMARY KEY,
    retention_days  INTEGER NOT NULL,
    archive_to_r2   INTEGER NOT NULL DEFAULT 0,
    last_cleanup_at  TEXT
);

-- Default retention policy
INSERT OR IGNORE INTO retention_policies (table_name, retention_days, archive_to_r2) VALUES
    ('check_results', 90, 1),
    ('incidents', 365, 1),
    ('audit_logs', 365, 1);
