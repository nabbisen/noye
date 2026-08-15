-- =============================================================
-- Subjects 17 (G-17, G-28, DEC-014) and 18 (G-13, G-14, G-15):
-- unreachable states removed, boolean/range/interval constraints
-- added, timestamp defaults fixed to RFC 3339, missing indexes
-- added. These constraints are final -- DEC-008 settled that no
-- tenant column is coming, so nothing here is reshaped later.
--
-- Application-level validation is necessary but not sufficient: data
-- reaches these tables through the API, the CLI, configuration
-- import, and direct database access, and a constraint that exists
-- only in Rust holds for one of those four.
--
-- SQLite cannot alter a column's CHECK or DEFAULT in place, so this
-- migration rebuilds nearly every table in the schema, using the
-- same create-copy-drop-rename procedure as 0004/0005/0006. Each
-- rebuild:
--   (a) adds the boolean/range/threshold CHECKs that table needs,
--   (b) replaces every `DEFAULT (datetime('now'))` with
--       `DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))` -- the
--       former produces "YYYY-MM-DD HH:MM:SS" (space, no 'Z'), which
--       this application's RFC 3339 "YYYY-MM-DDTHH:MM:SSZ" writes
--       compare against *as strings* (scheduling, window overlap):
--       ' ' is 0x20, 'T' is 0x54, so mixed formats sort incorrectly
--       but silently -- G-14, and the same shape of bug ruling 064
--       found in migration 0007 four days ago,
--   (c) normalises existing rows in the same column-copy, via
--       `strftime('%Y-%m-%dT%H:%M:%SZ', <col>)` -- verified directly
--       in sqlite3 that this correctly reformats both the old
--       space-separated form and an already-RFC-3339 value (a no-op
--       on the latter), and returns NULL for NULL rather than erroring.
--
-- ── The one column this does NOT normalise: audit_logs.action_time ──
--
-- audit_logs.row_hash = SHA256(prev_hash || canonical_serialization
-- (row)), and action_time is one of the fields the canonical
-- serialization covers (crates/core/src/db/audit/hash.rs,
-- canonical_row) -- confirmed directly, not assumed:
-- canonical_row_distinguishes_action_time asserts changing
-- action_time changes the hash. Reformatting an existing row's
-- action_time would change what canonical_row produces for that row
-- without changing its stored row_hash, and verify_chain would then
-- report every such row as tampered -- corrupting the tamper-evidence
-- chain in the act of fixing an unrelated defect. audit_logs.action_time
-- is therefore copied byte-for-byte unchanged (same discipline as
-- T-25/T-29c: classification-relevant columns preserved exactly across
-- a migration); only the DEFAULT clause changes, affecting rows
-- inserted after this migration. In practice this table's own INSERT
-- statements (db/audit.rs) always bind action_time explicitly in RFC
-- 3339 -- confirmed by reading every INSERT INTO audit_logs site -- so
-- the old DEFAULT was already dead code, never fired by the
-- application; fixing it going forward costs nothing and helps any
-- direct-SQL write that bypasses the app.
--
-- ── One thing NOT preserved: incidents.created_by ──
--
-- Subject 16 (migration 0008) split created_by into opened_by/
-- resolved_by and left the old column in place, unused, because 0008
-- adds columns and does not rebuild -- correctly, per the M2c
-- pre-flight's migration split. This rebuild is where it gets dropped
-- (ruling 064 §4.1): omitted from both the new table definition and
-- the INSERT ... SELECT.
--
-- ── Escalate ──
--
-- Per the handoff: a constraint rejecting rows already present in a
-- live database must be reported before anyone corrects the data.
-- This migration was developed and verified only against local
-- scratch/dev fixtures (standing rule 7 -- no real Cloudflare
-- infrastructure touched); no such rejection was found or is being
-- silently worked around.

-- ── users: is_active boolean CHECK; timestamp defaults fixed ──

CREATE TABLE users_new (
    id          TEXT PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

INSERT INTO users_new (id, email, name, role, is_active, created_at, updated_at)
SELECT id, email, name, role, is_active,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at),
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at)
  FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;
CREATE INDEX idx_users_email ON users(email);

-- ── targets: is_disabled boolean CHECK; range CHECKs (port,
-- expected_status, timeout_sec, retry_count, interval_minutes,
-- tls_threshold_days); threshold CHECKs (success/failure_threshold
-- BETWEEN 1 AND 10 -- zero must be unrepresentable, it would mean
-- "transition on no evidence"); timestamp defaults fixed. Preserves
-- 0006's shape (no tags column -- target_tags is the relation now)
-- and 0005's threshold columns. ──

CREATE TABLE targets_new (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    type             TEXT NOT NULL CHECK (type IN ('http', 'https', 'tcp', 'smtp', 'tls')),
    host             TEXT NOT NULL,
    port             INTEGER CHECK (port IS NULL OR port BETWEEN 1 AND 65535),
    path             TEXT DEFAULT '/',
    expected_status  INTEGER DEFAULT 200 CHECK (expected_status IS NULL OR expected_status BETWEEN 100 AND 599),
    body_contains    TEXT,
    tls_threshold_days INTEGER DEFAULT 30 CHECK (tls_threshold_days IS NULL OR tls_threshold_days >= 0),
    timeout_sec      INTEGER NOT NULL DEFAULT 10 CHECK (timeout_sec BETWEEN 1 AND 300),
    retry_count      INTEGER NOT NULL DEFAULT 3 CHECK (retry_count BETWEEN 0 AND 10),
    interval_minutes INTEGER NOT NULL DEFAULT 5 CHECK (interval_minutes BETWEEN 1 AND 1440),
    is_disabled      INTEGER NOT NULL DEFAULT 0 CHECK (is_disabled IN (0, 1)),
    owner_id         TEXT NOT NULL,
    next_check_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    created_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    created_by       TEXT NOT NULL,
    updated_by       TEXT NOT NULL,
    success_threshold  INTEGER NOT NULL DEFAULT 3 CHECK (success_threshold BETWEEN 1 AND 10),
    failure_threshold  INTEGER NOT NULL DEFAULT 3 CHECK (failure_threshold BETWEEN 1 AND 10),
    FOREIGN KEY(owner_id) REFERENCES users(id)
);

INSERT INTO targets_new
    (id, name, type, host, port, path, expected_status, body_contains,
     tls_threshold_days, timeout_sec, retry_count, interval_minutes,
     is_disabled, owner_id, next_check_at, created_at, updated_at,
     created_by, updated_by, success_threshold, failure_threshold)
SELECT id, name, type, host, port, path, expected_status, body_contains,
       tls_threshold_days, timeout_sec, retry_count, interval_minutes,
       is_disabled, owner_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', next_check_at),
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at),
       strftime('%Y-%m-%dT%H:%M:%SZ', updated_at),
       created_by, updated_by, success_threshold, failure_threshold
  FROM targets;

DROP TABLE targets;
ALTER TABLE targets_new RENAME TO targets;
CREATE INDEX idx_targets_next_check ON targets(next_check_at) WHERE is_disabled = 0;
CREATE INDEX idx_targets_owner ON targets(owner_id);
CREATE INDEX idx_targets_type ON targets(type);

-- ── notification_channels: is_enabled boolean CHECK; timestamp
-- default fixed; new index on owner_id (an FK with no supporting
-- index -- G-15). Rebuilt before target_notifications, which
-- references it. ──

CREATE TABLE notification_channels_new (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    channel_type    TEXT NOT NULL CHECK (channel_type IN ('webhook', 'email', 'slack')),
    endpoint        TEXT NOT NULL,
    is_enabled      INTEGER NOT NULL DEFAULT 1 CHECK (is_enabled IN (0, 1)),
    owner_id        TEXT NOT NULL,
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    FOREIGN KEY(owner_id) REFERENCES users(id)
);

INSERT INTO notification_channels_new (id, name, channel_type, endpoint, is_enabled, owner_id, created_at)
SELECT id, name, channel_type, endpoint, is_enabled, owner_id,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at)
  FROM notification_channels;

DROP TABLE notification_channels;
ALTER TABLE notification_channels_new RENAME TO notification_channels;
CREATE INDEX idx_channels_owner ON notification_channels(owner_id);

-- ── target_states (subject 17, G-28, DEC-014): current_status drops
-- 'degraded' and 'maintenance' -- decide_transition only ever produces
-- up/down, db/states.rs only ever writes what it produces, and
-- db/targets.rs no longer counts either (subject 17's Rust change).
-- consecutive_successes/consecutive_failures are counters, NOT
-- booleans -- deliberately given no CHECK; `CHECK (col IN (0,1))` on a
-- counter would break the monitor on the third consecutive failure,
-- with failure_threshold defaulting to 3, on precisely the transition
-- this product exists to detect. No timestamp DEFAULT columns exist
-- here (last_checked_at etc. are nullable, always application-written,
-- never schema-defaulted) -- nothing to normalise. ──

CREATE TABLE target_states_new (
    target_id               TEXT PRIMARY KEY,
    current_status          TEXT NOT NULL DEFAULT 'unknown'
                            CHECK (current_status IN ('up', 'down', 'unknown')),
    consecutive_successes   INTEGER NOT NULL DEFAULT 0,
    consecutive_failures    INTEGER NOT NULL DEFAULT 0,
    last_checked_at         TEXT,
    last_status_change_at   TEXT,
    last_notification_at    TEXT,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);

INSERT INTO target_states_new
    (target_id, current_status, consecutive_successes, consecutive_failures,
     last_checked_at, last_status_change_at, last_notification_at)
SELECT target_id, current_status, consecutive_successes, consecutive_failures,
       last_checked_at, last_status_change_at, last_notification_at
  FROM target_states;

DROP TABLE target_states;
ALTER TABLE target_states_new RENAME TO target_states;

-- ── check_results: is_success boolean CHECK; timestamp default fixed ──

CREATE TABLE check_results_new (
    id              TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL,
    checked_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    is_success      INTEGER NOT NULL CHECK (is_success IN (0, 1)),
    status_code     INTEGER,
    response_time_ms INTEGER,
    error_message   TEXT,
    tls_expiry_date TEXT,
    tls_days_left   INTEGER,
    details         TEXT,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);

INSERT INTO check_results_new
    (id, target_id, checked_at, is_success, status_code, response_time_ms,
     error_message, tls_expiry_date, tls_days_left, details)
SELECT id, target_id, strftime('%Y-%m-%dT%H:%M:%SZ', checked_at), is_success,
       status_code, response_time_ms, error_message, tls_expiry_date,
       tls_days_left, details
  FROM check_results;

DROP TABLE check_results;
ALTER TABLE check_results_new RENAME TO check_results;
CREATE INDEX idx_results_target_time ON check_results(target_id, checked_at DESC);
CREATE INDEX idx_results_checked_at ON check_results(checked_at);

-- ── incidents (subject 17, G-17, DEC-014): status drops
-- 'acknowledged' -- no code path produces it, no query reads it, no
-- interface offers it. Timestamp default fixed. Preserves subject
-- 15's idx_incident_one_open and subject 16's opened_by/resolved_by;
-- drops created_by (ruling 064 §4.1 -- 0008 left it in place, unused,
-- specifically because 0008 doesn't rebuild; this is where it goes). ──

CREATE TABLE incidents_new (
    id              TEXT PRIMARY KEY,
    target_id       TEXT NOT NULL,
    status          TEXT NOT NULL CHECK (status IN ('open', 'resolved')),
    opened_at       TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    resolved_at     TEXT,
    duration_sec    INTEGER,
    cause           TEXT,
    resolution_note TEXT,
    opened_by       TEXT,
    resolved_by     TEXT,
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE
);

INSERT INTO incidents_new
    (id, target_id, status, opened_at, resolved_at, duration_sec, cause,
     resolution_note, opened_by, resolved_by)
SELECT id, target_id, status, strftime('%Y-%m-%dT%H:%M:%SZ', opened_at),
       resolved_at, duration_sec, cause, resolution_note, opened_by, resolved_by
  FROM incidents;

DROP TABLE incidents;
ALTER TABLE incidents_new RENAME TO incidents;
CREATE INDEX idx_incidents_target ON incidents(target_id, opened_at DESC);
CREATE INDEX idx_incidents_status ON incidents(status);
CREATE UNIQUE INDEX idx_incident_one_open ON incidents(target_id) WHERE status = 'open';

-- ── maintenance_windows: three boolean CHECKs (suppress_notify,
-- exclude_from_sla, is_active); new CHECK (start_at < end_at) --
-- closes FR-SUP-10's schema half (G-13); timestamp default fixed.
-- Preserves subject 12's scope-exclusivity CHECK and idx_maint_active. ──

CREATE TABLE maintenance_windows_new (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    start_at        TEXT NOT NULL,
    end_at          TEXT NOT NULL,
    target_tag      TEXT,
    target_id       TEXT,
    suppress_notify INTEGER NOT NULL DEFAULT 1 CHECK (suppress_notify IN (0, 1)),
    exclude_from_sla INTEGER NOT NULL DEFAULT 1 CHECK (exclude_from_sla IN (0, 1)),
    is_active       INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    created_by      TEXT NOT NULL,
    updated_by      TEXT NOT NULL,
    CHECK (start_at < end_at),
    CHECK (NOT (target_id IS NOT NULL AND target_tag IS NOT NULL)),
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE SET NULL
);

INSERT INTO maintenance_windows_new
    (id, name, start_at, end_at, target_tag, target_id, suppress_notify,
     exclude_from_sla, is_active, created_at, created_by, updated_by)
SELECT id, name, start_at, end_at, target_tag, target_id, suppress_notify,
       exclude_from_sla, is_active,
       strftime('%Y-%m-%dT%H:%M:%SZ', created_at),
       created_by, updated_by
  FROM maintenance_windows;

DROP TABLE maintenance_windows;
ALTER TABLE maintenance_windows_new RENAME TO maintenance_windows;
CREATE INDEX idx_maint_active ON maintenance_windows(start_at, end_at) WHERE is_active = 1;

-- ── target_notifications: on_down/on_up boolean CHECKs. Neither is
-- read through bool_from_d1 (db/migration.rs reads them as i64,
-- compares != 0 -- safe, a 0/1 INTEGER is well inside D1's Number
-- range, not a latent G-36; not this subject's read path to change).
-- New reverse index on channel_id -- the primary key is
-- (target_id, channel_id), so a channel-to-targets lookup currently
-- scans (G-15). ──

CREATE TABLE target_notifications_new (
    target_id   TEXT NOT NULL,
    channel_id  TEXT NOT NULL,
    on_down     INTEGER NOT NULL DEFAULT 1 CHECK (on_down IN (0, 1)),
    on_up       INTEGER NOT NULL DEFAULT 1 CHECK (on_up IN (0, 1)),
    PRIMARY KEY(target_id, channel_id),
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE CASCADE,
    FOREIGN KEY(channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE
);

INSERT INTO target_notifications_new (target_id, channel_id, on_down, on_up)
SELECT target_id, channel_id, on_down, on_up
  FROM target_notifications;

DROP TABLE target_notifications;
ALTER TABLE target_notifications_new RENAME TO target_notifications;
CREATE INDEX idx_target_notifications_channel ON target_notifications(channel_id);

-- ── audit_logs: timestamp DEFAULT fixed for future inserts only --
-- action_time itself is copied byte-for-byte, NOT normalised; see this
-- file's header for why (row_hash covers action_time; reformatting an
-- existing row's action_time would desynchronise it from its own
-- stored hash). actor_id's CHECK (!= '') and result's CHECK are
-- unchanged and carried across. New index on action_type (G-15);
-- the other four existing indexes are preserved. ──

CREATE TABLE audit_logs_new (
    id              TEXT PRIMARY KEY,
    action_time     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    actor_id        TEXT NOT NULL CHECK (actor_id != ''),
    actor_email     TEXT,
    resource_type   TEXT NOT NULL,
    resource_id     TEXT,
    action_type     TEXT NOT NULL,
    previous_value  TEXT,
    new_value       TEXT,
    result          TEXT NOT NULL DEFAULT 'success' CHECK (result IN ('success', 'failure')),
    ip_address      TEXT,
    prev_hash       TEXT,
    row_hash        TEXT
);

INSERT INTO audit_logs_new
    (id, action_time, actor_id, actor_email, resource_type, resource_id,
     action_type, previous_value, new_value, result, ip_address, prev_hash, row_hash)
SELECT id, action_time, actor_id, actor_email, resource_type, resource_id,
       action_type, previous_value, new_value, result, ip_address, prev_hash, row_hash
  FROM audit_logs;

DROP TABLE audit_logs;
ALTER TABLE audit_logs_new RENAME TO audit_logs;
CREATE INDEX idx_audit_time ON audit_logs(action_time DESC);
CREATE INDEX idx_audit_actor ON audit_logs(actor_id);
CREATE INDEX idx_audit_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_row_hash ON audit_logs(row_hash);
CREATE INDEX idx_audit_action_type ON audit_logs(action_type);

-- ── retention_policies: archive_to_r2 boolean CHECK. last_cleanup_at
-- is nullable with no DEFAULT (application-written only) -- nothing to
-- normalise. ──

CREATE TABLE retention_policies_new (
    table_name      TEXT PRIMARY KEY,
    retention_days  INTEGER NOT NULL,
    archive_to_r2   INTEGER NOT NULL DEFAULT 0 CHECK (archive_to_r2 IN (0, 1)),
    last_cleanup_at  TEXT
);

INSERT INTO retention_policies_new (table_name, retention_days, archive_to_r2, last_cleanup_at)
SELECT table_name, retention_days, archive_to_r2, last_cleanup_at
  FROM retention_policies;

DROP TABLE retention_policies;
ALTER TABLE retention_policies_new RENAME TO retention_policies;
