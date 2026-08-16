-- =============================================================
-- Subject 19 (G-16): identity resolves by email only, which is
-- case-sensitive in SQLite by default. A provider returning different
-- casing for the same address creates a duplicate account for one
-- person; an email change at the identity provider orphans the
-- account. Both are exactly what FR-RBAC-07 exists to prevent.
-- =============================================================

-- Steps 1 and 3 land as one rebuild, not an ALTER TABLE followed by a
-- rebuild: SQLite's ALTER TABLE ADD COLUMN refuses a UNIQUE column
-- outright ("Cannot add a UNIQUE column", confirmed directly against
-- real sqlite3) -- a unique column can only be declared in CREATE
-- TABLE. Since step 3 (the email collation change) already requires a
-- full table-rebuild, sub is declared there too, in the same
-- CREATE TABLE, rather than as a separate ALTER TABLE this database
-- would then refuse.
--
-- sub TEXT UNIQUE permits multiple NULLs -- verified directly against
-- real sqlite3 that SQLite's UNIQUE treats every NULL as distinct
-- from every other NULL, so every existing row copied across as
-- unclaimed (NULL) below does not conflict with any other. The
-- application backfills it on next login
-- (crates/core/src/db/users.rs::resolve_by_identity): sub is tried
-- first, falling back to email exactly once per row to match and
-- claim it, never a row whose sub is already claimed by someone else.
-- Do not tighten this column to NOT NULL -- that breaks every account
-- created before this migration, which is precisely the backfill
-- design's premise.
--
-- email becomes case-insensitive-unique. SQLite has no way to
-- change a column's collation in place, so this rebuilds `users` --
-- the same table migration 0009 rebuilt one milestone ago. Every
-- constraint and default it added is carried forward (role's CHECK,
-- is_active's CHECK and default, created_at/updated_at's RFC 3339
-- defaults) -- guarded by T-98b, which reads sqlite_master rather
-- than re-evaluating an expression (subject 18's own T-91 shipped
-- unable to detect a reverted default; T-98b is written against the
-- lesson, not just the requirement).
--
-- Pre-existing duplicate emails that differ only by casing are NOT
-- resolved by this migration. Subject 15's precedent (auto-resolving
-- duplicate incidents) does not transfer: two incident rows for one
-- target are one event recorded twice, but two user rows are two
-- identity records for one person, and merging them means choosing
-- which id survives. Six foreign keys reference users(id) (three in
-- 0001, one in 0006, two in 0009), covering target and channel
-- ownership -- a merge would silently reassign owned resources. And
-- audit_logs carries an actor *snapshot* (actor_id, actor_email,
-- subject 06), deliberately not a foreign key, so history would keep
-- pointing at whichever id this migration discarded -- unattributable,
-- in the record whose entire purpose is attribution. Which account
-- survives is an operator decision, not a migration's.
--
-- The INSERT below fails outright (UNIQUE constraint violation) if
-- any pre-existing rows collide case-insensitively, which fails this
-- entire migration atomically -- every statement in this file runs in
-- one transaction (scripts/check-migrations.sh's apply_sql_file, and
-- D1's own real migration application, subject 06's 020 §2a), so
-- nothing partially applies.
--
-- If this migration fails with "UNIQUE constraint failed:
-- users_new.email", find the conflicting addresses before doing
-- anything else -- do not retry, and do not pick a survivor without
-- looking:
--   SELECT email FROM users GROUP BY email COLLATE NOCASE HAVING COUNT(*) > 1;
-- Resolving which account survives (reassigning or documenting the
-- fate of owned targets/channels, and deciding what the audit
-- trail's now-orphaned actor references should mean) is an operator
-- decision to make deliberately, then reapply this migration.
CREATE TABLE users_new (
    id          TEXT PRIMARY KEY,
    email       TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name        TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    sub         TEXT UNIQUE
);

-- The source `users` table has no `sub` column at all yet -- every
-- row copied across starts unclaimed (NULL), to be backfilled on that
-- person's next login.
INSERT INTO users_new (id, email, name, role, is_active, created_at, updated_at, sub)
SELECT id, email, name, role, is_active, created_at, updated_at, NULL
  FROM users;

DROP TABLE users;
ALTER TABLE users_new RENAME TO users;
CREATE INDEX idx_users_email ON users(email);
