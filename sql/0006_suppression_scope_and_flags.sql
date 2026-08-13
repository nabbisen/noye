-- =============================================================
-- Subjects 11 (G-07), 12 (G-08, G-09, G-27): suppression windows
-- gain a second flag, and tag scope becomes exact. Three parts.
-- =============================================================

-- ── Part 1 (subject 11, G-07): a second flag ──
--
-- suppress_notify and exclude_from_sla become independent -- DEC-013's
-- three named situations (Planned maintenance / Known external outage
-- / Expected noise). Defaults to 1 (excluded), matching today's
-- *intended* -- if not actually implemented -- behaviour, so no
-- existing window's meaning changes at migration time.
--
-- This is a bool backed by an INTEGER, and that is G-36: the shared
-- MaintenanceWindow struct carries #[serde(deserialize_with =
-- "bool_from_d1")] on this field (see crates/shared/src/lib.rs) --
-- without it, every read of maintenance_windows traps. See
-- docs/src/d1-type-boundary.md.
ALTER TABLE maintenance_windows ADD COLUMN exclude_from_sla INTEGER NOT NULL DEFAULT 1;

-- ── Part 2 (subject 12, G-09/G-27): tag scope becomes a relation ──
--
-- targets.tags was a JSON-array-encoded TEXT column, matched by
-- `LIKE '%' || target_tag || '%'` -- substring (G-09) with the stored
-- tag on the *pattern* side, so `%`/`_` in a tag act as wildcards
-- (G-27). A target_tags relation makes exact matching a join instead
-- of a string comparison, and makes both defects structurally
-- impossible rather than merely fixed today.
--
-- json_each() raises a SQL error on malformed JSON rather than
-- silently skipping it -- backfilling against actually-malformed
-- `targets.tags` content fails this migration loudly, which is the
-- point: report it, don't drop tags quietly. INSERT OR IGNORE only
-- covers a legitimate duplicate tag within one target's own array,
-- which is harmless (same tag twice is the same tag), not the
-- malformed-input case.
CREATE TABLE target_tags (
    target_id TEXT NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
    tag       TEXT NOT NULL,
    PRIMARY KEY (target_id, tag)
);
CREATE INDEX idx_target_tags_tag ON target_tags(tag);

INSERT OR IGNORE INTO target_tags (target_id, tag)
SELECT t.id, je.value
FROM targets t, json_each(t.tags) je
WHERE t.tags IS NOT NULL AND t.tags != '';

-- Drop targets.tags -- noye_shared::Target keeps `tags` as a *derived*
-- field from here on (computed from target_tags on read, consumed
-- into it on write); the configuration document's wire shape does not
-- change (subject 12's explicit scope). Standard SQLite rebuild, same
-- shape as 0004/0005: create the replacement table without the
-- column, copy every row, drop, rename, recreate indexes.
CREATE TABLE targets_new (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    type             TEXT NOT NULL CHECK (type IN ('http', 'https', 'tcp', 'smtp', 'tls')),
    host             TEXT NOT NULL,
    port             INTEGER,
    path             TEXT DEFAULT '/',
    expected_status  INTEGER DEFAULT 200,
    body_contains    TEXT,
    tls_threshold_days INTEGER DEFAULT 30,
    timeout_sec      INTEGER NOT NULL DEFAULT 10,
    retry_count      INTEGER NOT NULL DEFAULT 3,
    interval_minutes INTEGER NOT NULL DEFAULT 5,
    is_disabled      INTEGER NOT NULL DEFAULT 0,
    owner_id         TEXT NOT NULL,
    next_check_at    TEXT NOT NULL DEFAULT (datetime('now')),
    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at       TEXT NOT NULL DEFAULT (datetime('now')),
    created_by       TEXT NOT NULL,
    updated_by       TEXT NOT NULL,
    success_threshold  INTEGER NOT NULL DEFAULT 3,
    failure_threshold  INTEGER NOT NULL DEFAULT 3,
    FOREIGN KEY(owner_id) REFERENCES users(id)
);

INSERT INTO targets_new
    (id, name, type, host, port, path, expected_status, body_contains,
     tls_threshold_days, timeout_sec, retry_count, interval_minutes,
     is_disabled, owner_id, next_check_at, created_at, updated_at,
     created_by, updated_by, success_threshold, failure_threshold)
SELECT id, name, type, host, port, path, expected_status, body_contains,
       tls_threshold_days, timeout_sec, retry_count, interval_minutes,
       is_disabled, owner_id, next_check_at, created_at, updated_at,
       created_by, updated_by, success_threshold, failure_threshold
  FROM targets;

DROP TABLE targets;
ALTER TABLE targets_new RENAME TO targets;

CREATE INDEX idx_targets_next_check ON targets(next_check_at) WHERE is_disabled = 0;
CREATE INDEX idx_targets_owner ON targets(owner_id);
CREATE INDEX idx_targets_type ON targets(type);

-- ── Part 3 (subject 12, G-08): scope exclusivity ──
--
-- FR-SUP-03 says target scope beats tag scope. Encoding that as
-- precedence logic means two queries (is_under_maintenance,
-- list_in_window) must agree forever. A CHECK constraint makes the
-- ambiguous state unrepresentable instead, which cannot drift.
--
-- Resolution policy for existing rows that violate it (target scope
-- wins, matching FR-SUP-03's own precedence rule): clear target_tag
-- wherever both target_id and target_tag are set. Applied before the
-- constraint is added, so the migration itself never fails on data it
-- is about to make illegal.
UPDATE maintenance_windows
   SET target_tag = NULL
 WHERE target_id IS NOT NULL AND target_tag IS NOT NULL;

CREATE TABLE maintenance_windows_new (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    start_at        TEXT NOT NULL,
    end_at          TEXT NOT NULL,
    target_tag      TEXT,
    target_id       TEXT,
    suppress_notify INTEGER NOT NULL DEFAULT 1,
    exclude_from_sla INTEGER NOT NULL DEFAULT 1,
    is_active       INTEGER NOT NULL DEFAULT 1,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    created_by      TEXT NOT NULL,
    updated_by      TEXT NOT NULL,
    CHECK (NOT (target_id IS NOT NULL AND target_tag IS NOT NULL)),
    FOREIGN KEY(target_id) REFERENCES targets(id) ON DELETE SET NULL
);

INSERT INTO maintenance_windows_new
    (id, name, start_at, end_at, target_tag, target_id, suppress_notify,
     exclude_from_sla, is_active, created_at, created_by, updated_by)
SELECT id, name, start_at, end_at, target_tag, target_id, suppress_notify,
       exclude_from_sla, is_active, created_at, created_by, updated_by
  FROM maintenance_windows;

DROP TABLE maintenance_windows;
ALTER TABLE maintenance_windows_new RENAME TO maintenance_windows;

CREATE INDEX idx_maint_active ON maintenance_windows(start_at, end_at) WHERE is_active = 1;
