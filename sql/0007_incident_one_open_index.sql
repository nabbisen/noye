-- Subject 15 (G-11): at most one open incident per target, enforced by
-- the database rather than application flow alone -- re-entrant
-- scheduling, manual operations, or any future concurrency could
-- otherwise produce duplicates with nothing to stop them.

-- Resolve pre-existing duplicates first, so the unique index below can
-- be created without refusing to apply. Ranks each target's *open*
-- incidents by opened_at (ties broken by id, for determinism); the
-- earliest (rank 1) is kept open -- it's the one actually tracking the
-- ongoing outage -- and every later duplicate (rank > 1) is
-- force-resolved with a resolution_note explaining why, and a
-- duration_sec computed the same way subject 14's auto-resolve
-- computes one (opened_at to the resolution instant, in seconds).
-- resolved_at uses strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), not
-- datetime('now') -- the latter produces "YYYY-MM-DD HH:MM:SS"
-- (space, no 'Z'), not this application's RFC 3339
-- "YYYY-MM-DDTHH:MM:SSZ". That mismatch is G-14's own shape, and
-- db/incidents.rs's SLA/MTTR window filter (`resolved_at > ?2`) is a
-- *string* comparison: ' ' (0x20) sorts before 'T' (0x54), so a row
-- carrying the space-separated form would silently fail that
-- comparison and drop out of every report window that should have
-- included it -- confirmed directly in sqlite3, not assumed.
UPDATE incidents
   SET status = 'resolved',
       resolved_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'),
       duration_sec = CAST(strftime('%s', 'now') AS INTEGER)
                       - CAST(strftime('%s', opened_at) AS INTEGER),
       resolution_note = 'auto-resolved: duplicate open incident found during migration 0007'
 WHERE id IN (
     SELECT id FROM (
         SELECT id, ROW_NUMBER() OVER (
             PARTITION BY target_id ORDER BY opened_at ASC, id ASC
         ) AS rn
         FROM incidents WHERE status = 'open'
     )
     WHERE rn > 1
 );

-- Per DEC-014 the index covers 'open' alone; 'acknowledged' is removed
-- in a later subject (17) and never participated in this constraint.
CREATE UNIQUE INDEX idx_incident_one_open
    ON incidents(target_id) WHERE status = 'open';
