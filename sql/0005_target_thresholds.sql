-- =============================================================
-- Subject 10 (G-06): target thresholds move from target_states
-- to targets. RFC 0008 (accepted, DEC-012).
-- =============================================================
--
-- success_threshold/failure_threshold lived on the state row, but they
-- are configuration, not state -- every other decision criterion
-- (expected_status, body_contains, tls_threshold_days, timeout_sec,
-- retry_count, interval_minutes) already lives on targets. Because the
-- configuration document is built from Target, not TargetState,
-- thresholds were never exported: an import round trip silently reset
-- a deliberately-configured threshold back to the schema default (3),
-- and the import fix (subject 08/09) had no state row to create a
-- threshold onto in the first place.
--
-- Step 1-2: add the columns to targets, defaulting to 3 -- the same
-- default target_states already carried, so no existing target's
-- behaviour changes at migration time.
-- Step 3: copy each target's current threshold values across from its
-- state row before that row's columns are dropped.
-- Step 4: rebuild target_states without them, via the standard SQLite
-- table-rebuild procedure (no ALTER TABLE ... DROP COLUMN shortcut is
-- used, to keep this migration's shape consistent with 0004's). Once
-- this lands, target_states carries only genuine state -- delete a row
-- and monitoring rebuilds it from the next check.

ALTER TABLE targets ADD COLUMN success_threshold INTEGER NOT NULL DEFAULT 3;
ALTER TABLE targets ADD COLUMN failure_threshold INTEGER NOT NULL DEFAULT 3;

UPDATE targets
SET success_threshold = (
        SELECT success_threshold FROM target_states
        WHERE target_states.target_id = targets.id
    ),
    failure_threshold = (
        SELECT failure_threshold FROM target_states
        WHERE target_states.target_id = targets.id
    )
WHERE EXISTS (
    SELECT 1 FROM target_states WHERE target_states.target_id = targets.id
);

CREATE TABLE target_states_new (
    target_id               TEXT PRIMARY KEY,
    current_status          TEXT NOT NULL DEFAULT 'unknown'
                            CHECK (current_status IN ('up', 'down', 'degraded', 'maintenance', 'unknown')),
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
