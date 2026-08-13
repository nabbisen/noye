-- Subject 16 (G-29): incidents.created_by carried two meanings across a
-- row's lifetime -- "who opened it" until resolved, then overwritten to
-- "who resolved it" -- so the incident CSV's created_by column meant
-- different things for open and resolved rows, and a consumer parsing
-- the export could not tell which. Split into opened_by and resolved_by.
--
-- created_by is left in place, unused from here on by application code
-- (a later subject rebuilds this table for unrelated CHECK-constraint
-- reasons and drops it then; this migration only adds columns).
ALTER TABLE incidents ADD COLUMN opened_by TEXT;
ALTER TABLE incidents ADD COLUMN resolved_by TEXT;

-- Backfill from the ground truth, not from created_by's current value:
-- db/incidents.rs's open() takes no caller and has only ever written
-- the literal 'system' -- no route opens an incident manually -- so
-- every existing row, open or resolved, was opened by 'system'
-- regardless of what created_by currently holds (it may already have
-- been overwritten by a resolve).
UPDATE incidents SET opened_by = 'system';

-- resolved_by backfills from created_by's *current* value, but only for
-- rows resolve() has actually touched -- i.e. anything not still open.
-- For those rows created_by currently holds the resolver's identity
-- (resolve() overwrites it; auto_resolve() sets it to the literal
-- 'system'), which is exactly what resolved_by should carry.
UPDATE incidents SET resolved_by = created_by WHERE status != 'open';
