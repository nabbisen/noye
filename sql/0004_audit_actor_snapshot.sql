-- =============================================================
-- Subject 06 (G-03): system-actor audit rows can be written.
-- =============================================================
--
-- audit_logs.actor_id was NOT NULL with a foreign key to users(id).
-- log_system writes the sentinel actor "system", for which no user
-- row exists, so the insert fails and the caller discards the
-- result -- system-originated audit events were silently absent, and
-- the chain still verified, since it only covers rows that exist.
--
-- The standard SQLite table-rebuild (SQLite has no ALTER TABLE that
-- drops a foreign key directly): create a replacement table without
-- the foreign key, carrying an explicit non-empty check instead, copy
-- every row across, drop the old table, rename, recreate the indexes.
--
-- The actor is now a snapshot (actor_id, actor_email captured at
-- write time), not a live reference -- see
-- rfcs/handoffs/06-audit-actor-snapshot.md, docs/src/architecture.md,
-- docs/src/security-posture.md.
--
-- Scope, per DEC-021: this migration serves Class B and Class C
-- databases (audit_logs already carries prev_hash/row_hash) and
-- copies them directly and unconditionally. Class A (provisioned from
-- sql/0001_initial.sql as it stood at tag 0.1.0, no hash columns) is
-- assumed not to exist -- an accepted assumption, not a verified
-- fact, per scripts/classify-audit-schema.sh's own access boundary
-- (rfcs/handoffs/README.md standing rule 7). This assumption fails
-- safe: naming prev_hash/row_hash against a source lacking them fails
-- at prepare, before any statement runs, and this migration file is
-- all-or-nothing -- a Class A database is left completely untouched,
-- 0004 stays pending, and assert_hash_columns_present continues
-- refusing service with a named, actionable error in the meantime.

CREATE TABLE audit_logs_new (
    id              TEXT PRIMARY KEY,
    action_time     TEXT NOT NULL DEFAULT (datetime('now')),
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
    -- No FOREIGN KEY(actor_id) REFERENCES users(id). The actor is
    -- recorded as a snapshot at write time (id and, where known,
    -- email), not a live reference: a "system" actor, or a later
    -- deactivated or renamed user, must not invalidate history that
    -- already happened, and must not silently fail to record.
);

-- Explicit column list, never SELECT * -- a Class A source has 11
-- columns, not 13, and SELECT * would supply the wrong number of
-- values into audit_logs_new (a parse error, not a graceful skip).
-- Naming prev_hash/row_hash here is exactly what makes this fail at
-- prepare, safely, against a Class A source -- see the header above.
INSERT INTO audit_logs_new
    (id, action_time, actor_id, actor_email, resource_type,
     resource_id, action_type, previous_value, new_value,
     result, ip_address, prev_hash, row_hash)
SELECT id, action_time, actor_id, actor_email, resource_type,
       resource_id, action_type, previous_value, new_value,
       result, ip_address, prev_hash, row_hash
  FROM audit_logs;

DROP TABLE audit_logs;
ALTER TABLE audit_logs_new RENAME TO audit_logs;

CREATE INDEX idx_audit_time ON audit_logs(action_time DESC);
CREATE INDEX idx_audit_actor ON audit_logs(actor_id);
CREATE INDEX idx_audit_resource ON audit_logs(resource_type, resource_id);
CREATE INDEX idx_audit_row_hash ON audit_logs(row_hash);
