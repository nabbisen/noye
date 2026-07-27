-- Migration 0002: Add hash-chain columns to audit_logs.
--
-- Each row now carries:
--   prev_hash  - the row_hash of the immediately prior row (or 64 hex zeros
--                for the genesis row), enabling chain traversal.
--   row_hash   - SHA-256 over (prev_hash || canonical_serialization(row)),
--                pinning the row's content to its position in the chain.
--
-- Rows written before this migration have both columns NULL. The
-- verification routine in crates/core/src/db/audit.rs treats them as
-- "legacy rows" and skips them; the chain begins fresh with the next INSERT.
--
-- See docs/security-posture.md#audit-logging for the threat model and
-- crates/core/src/db/audit/hash.rs for the canonical serialization format.

ALTER TABLE audit_logs ADD COLUMN prev_hash TEXT;
ALTER TABLE audit_logs ADD COLUMN row_hash  TEXT;

-- Index supports verifier-side lookups by row_hash if a future tool wants
-- to check whether a specific hash exists anywhere in the chain (e.g. an
-- off-system mirror reconciling against the live table).
CREATE INDEX IF NOT EXISTS idx_audit_row_hash ON audit_logs(row_hash);
