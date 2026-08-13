#!/usr/bin/env bash
# Migration-apply gate (subject: rfcs/handoffs/01-migration-applicability.md).
#
# Verifies, against a fresh SQLite database (no D1 or Wrangler needed):
#
#   T-01 — every file in sql/*.sql applies, in filename order, without error
#   T-02 — audit_logs ends up with prev_hash, row_hash, idx_audit_row_hash
#   T-03 — a deliberately broken migration fails this gate, not just the DDL
#   T-01a — a database that predates the hash-chain columns (tag 0.1.0,
#           "Class A" in requirements.md G-01) is reported, not silently
#           treated as healthy
#   T-17 — retention_policies has no audit_logs row after migration
#           (subject 04, G-04)
#   T-24 — a log_system-shaped insert (actor "system") against zero
#           users fails before migration 0004, succeeds after
#           (subject 06, G-03)
#   T-25 — every classification-relevant column is preserved
#           byte-for-byte across 0004 (subject 06)
#   T-26 — all four audit_logs indexes exist after 0004
#   T-27 — an audit row with an empty actor_id is rejected after 0004
#           (subject 06)
#   T-28 — deactivating or renaming a user alters no historical audit
#           row (subject 06)
#   T-29 — monitor/engine.rs's two log_system calls (status_down,
#           status_up) still exist and now produce rows (subject 06)
#   T-29a — 0004 refuses to apply to a Class A fixture (no such
#           column: prev_hash) and leaves it untouched (subject 06,
#           DEC-021)
#   T-29c — prev_hash/row_hash preserved byte-for-byte across 0004
#           (subject 06)
#
# T-01 and T-01a are must-fail-first: run this script against the pre-fix
# tree (before sql/0002_audit_hash_chain.sql was deleted, or against a
# tree with no Class A detection) and it fails. See
# .git-exclude/evidence/baseline-p0-p1.log for that capture. T-17 is
# must-fail-first against the tree predating sql/0003 — see
# .git-exclude/evidence/baseline-04.log. T-24, T-27, T-29 and T-29a are
# must-fail-first against the tree predating sql/0004 — see
# .git-exclude/evidence/baseline-06.log.
#
# T-24 sets `PRAGMA foreign_keys=ON` explicitly before the insert.
# Bare sqlite3 defaults this OFF, which is the wrong-answer trap
# rfcs/handoffs/06-audit-actor-snapshot.md warns about (a `sqlite3`
# reproduction with the pragma left at its default tells you the
# insert succeeds, which is false for D1). Confirmed against real D1
# during Subject 06 Step 0 (`wrangler d1 execute --local`): D1's
# actual default is `PRAGMA foreign_keys = 1` — setting it explicitly
# here reproduces that, not sqlite3's own default.
#
# Every migration-file application in this script goes through
# apply_sql_file (subject 07a Build step 4), which wraps the file in
# an explicit transaction and runs it under `sqlite3 -bail` -- the
# gate's default now, not just T-29a's local fix. Bare `sqlite3 db <
# file.sql` keeps executing statements after one fails, so a mid-file
# failure can leave later statements' effects committed while the exit
# code still reports failure; D1's real migration application is
# all-or-nothing per file (subject 06's `020` §2a). There is no live
# defect from this today -- T-01 aborts the whole gate on the first
# nonzero exit, before anything downstream reads the database -- but
# the untouched half of a fail-safe claim (T-29a) is not real evidence
# unless the reproduction actually models D1's atomicity, and the
# naive form was found completing a rename it should have refused.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SQL_DIR="$REPO_ROOT/sql"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# Apply one SQL file to a database the way D1 actually applies a
# migration: wrapped in a single transaction, aborting at the first
# error rather than printing it and continuing (subject 07a Build
# step 4). Bare `sqlite3 db < file.sql` does neither -- it keeps
# executing statements after one fails, so a mid-file failure can
# leave later statements' effects committed while still reporting
# nonzero. Subject 06's T-29a found this directly: the naive form
# reported "0004 failed" against a Class A fixture while silently
# completing the DROP/RENAME/CREATE INDEX anyway, which would have
# made that test pass on a false premise. Confirmed clean of PRAGMA/
# VACUUM statements across sql/*.sql, so wrapping every application in
# a transaction is safe.
apply_sql_file() {
  local db="$1" file="$2" errfile="$3"
  { printf 'BEGIN IMMEDIATE;\n'; cat "$file"; printf 'COMMIT;\n'; } \
    | sqlite3 -bail "$db" 2>"$errfile"
}

# ── T-01 — apply every sql/*.sql, in filename order, to a fresh database ──
FRESH_DB="$WORKDIR/fresh.db"
for f in $(find "$SQL_DIR" -maxdepth 1 -name '*.sql' | sort); do
  if ! apply_sql_file "$FRESH_DB" "$f" "$WORKDIR/apply.err"; then
    cat "$WORKDIR/apply.err" >&2
    fail "T-01: $f did not apply cleanly to a fresh database"
  fi
done
echo "PASS T-01: all migrations applied to a fresh database"

# ── T-02 — the hash-chain columns and index exist afterward ──
COLS="$(sqlite3 "$FRESH_DB" "PRAGMA table_info(audit_logs);" | grep -c -E '\|(prev_hash|row_hash)\|')"
[ "$COLS" -eq 2 ] || fail "T-02: expected prev_hash and row_hash on audit_logs, found $COLS"
IDX="$(sqlite3 "$FRESH_DB" "SELECT name FROM sqlite_master WHERE type='index' AND name='idx_audit_row_hash';")"
[ "$IDX" = "idx_audit_row_hash" ] || fail "T-02: idx_audit_row_hash is missing"
echo "PASS T-02: prev_hash, row_hash, idx_audit_row_hash present"

# ── T-17 — no audit_logs row in retention_policies (subject 04, G-04) ──
AUDIT_POLICY_COUNT="$(sqlite3 "$FRESH_DB" "SELECT count(*) FROM retention_policies WHERE table_name='audit_logs';")"
[ "$AUDIT_POLICY_COUNT" -eq 0 ] || fail "T-17: expected no audit_logs row in retention_policies, found $AUDIT_POLICY_COUNT"
echo "PASS T-17: retention_policies has no audit_logs row after migration"

# ── Subject 06 (G-03) fixtures ──
GENESIS="0000000000000000000000000000000000000000000000000000000000000000"

# A database with 0001+0003 only -- audit_logs still has the actor_id
# foreign key. Used for T-24's must-fail-first half.
PRE_0004_DB="$WORKDIR/pre-0004.db"
apply_sql_file "$PRE_0004_DB" "$SQL_DIR/0001_initial.sql" "$WORKDIR/setup-0001.err" || fail "setup: 0001 failed to apply to the pre-0004 fixture: $(cat "$WORKDIR/setup-0001.err")"
apply_sql_file "$PRE_0004_DB" "$SQL_DIR/0003_audit_retention_exemption.sql" "$WORKDIR/setup-0003.err" || fail "setup: 0003 failed to apply to the pre-0004 fixture: $(cat "$WORKDIR/setup-0003.err")"

LOG_SYSTEM_INSERT="INSERT INTO audit_logs (id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, new_value, result, prev_hash, row_hash) VALUES ('t24-row', '2026-01-01T00:00:00Z', 'system', 'system', 'target', 'x', 'status_down', NULL, 'success', '$GENESIS', 'deadbeef');"

# ── T-24 — log_system-shaped insert against zero users: fails pre-0004, succeeds after ──
if printf 'PRAGMA foreign_keys=ON;\n%s\n' "$LOG_SYSTEM_INSERT" | sqlite3 "$PRE_0004_DB" 2>/dev/null; then
  fail "T-24 baseline: expected a log_system-shaped insert against zero users to fail before 0004, but it succeeded"
fi
echo "PASS T-24 baseline (must-fail-first): pre-0004, the insert fails (foreign key)"

POST_0004_T24_DB="$WORKDIR/post-0004-t24.db"
cp "$FRESH_DB" "$POST_0004_T24_DB"
if ! printf 'PRAGMA foreign_keys=ON;\n%s\n' "$LOG_SYSTEM_INSERT" | sqlite3 "$POST_0004_T24_DB" 2>"$WORKDIR/t24.err"; then
  cat "$WORKDIR/t24.err" >&2
  fail "T-24: the same insert still fails after 0004"
fi
echo "PASS T-24: after 0004, the same insert succeeds"

# ── T-29 — monitor/engine.rs's two log_system calls (status_down line
#    167, status_up line 188) now produce rows. log_system's INSERT
#    (crates/core/src/db/audit.rs) is one fixed SQL statement whose
#    success depends only on schema, not on which caller invoked it --
#    the D1-behavior half of that is already what T-24 proves. What T-24
#    alone does not guard is the wiring: that engine.rs still calls
#    log_system, unconditionally, from both the "down" and "up"
#    branches. Guard both: the source still contains the call next to
#    each literal action_type (regression if a future edit drops it),
#    and the exact argument shape each call site passes reproduces
#    T-24's fail-before/succeed-after result. ──
ENGINE_RS="$REPO_ROOT/crates/core/src/monitor/engine.rs"
grep -B6 '"status_down"' "$ENGINE_RS" | grep -q 'db::audit::log_system' \
  || fail "T-29: no db::audit::log_system call found near the status_down action_type in monitor/engine.rs -- the \"down\" branch's audit call may have been removed"
grep -B6 '"status_up"' "$ENGINE_RS" | grep -q 'db::audit::log_system' \
  || fail "T-29: no db::audit::log_system call found near the status_up action_type in monitor/engine.rs -- the \"up\" branch's audit call may have been removed"
echo "PASS T-29 (source guard): monitor/engine.rs still calls log_system from both the down and up branches"

STATUS_DOWN_INSERT="INSERT INTO audit_logs (id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, new_value, result, prev_hash, row_hash) VALUES ('t29-down', '2026-01-01T00:00:00Z', 'system', 'system', 'target', 't-1', 'status_down', 'Connection refused', 'success', '$GENESIS', 'hash-t29-down');"
STATUS_UP_INSERT="INSERT INTO audit_logs (id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, new_value, result, prev_hash, row_hash) VALUES ('t29-up', '2026-01-01T00:00:01Z', 'system', 'system', 'target', 't-1', 'status_up', 'Auto-recovered', 'success', 'hash-t29-down', 'hash-t29-up');"

if printf 'PRAGMA foreign_keys=ON;\n%s\n%s\n' "$STATUS_DOWN_INSERT" "$STATUS_UP_INSERT" | sqlite3 "$PRE_0004_DB" 2>/dev/null; then
  fail "T-29 baseline: expected engine.rs's status_down/status_up inserts to fail against zero users before 0004, but they succeeded"
fi
echo "PASS T-29 baseline (must-fail-first): pre-0004, engine.rs's audit inserts fail (foreign key)"

POST_0004_T29_DB="$WORKDIR/post-0004-t29.db"
cp "$FRESH_DB" "$POST_0004_T29_DB"
if ! printf 'PRAGMA foreign_keys=ON;\n%s\n%s\n' "$STATUS_DOWN_INSERT" "$STATUS_UP_INSERT" | sqlite3 "$POST_0004_T29_DB" 2>"$WORKDIR/t29.err"; then
  cat "$WORKDIR/t29.err" >&2
  fail "T-29: engine.rs's status_down/status_up inserts still fail after 0004"
fi
T29_ROWS="$(sqlite3 "$POST_0004_T29_DB" "SELECT count(*) FROM audit_logs WHERE id IN ('t29-down','t29-up');")"
[ "$T29_ROWS" -eq 2 ] || fail "T-29: expected both the status_down and status_up rows to exist after 0004, found $T29_ROWS"
echo "PASS T-29: after 0004, both engine.rs audit inserts succeed"

# ── T-26 — all four audit_logs indexes exist after 0004 ──
EXPECTED_INDEXES="idx_audit_actor idx_audit_resource idx_audit_row_hash idx_audit_time"
ACTUAL_INDEXES="$(sqlite3 "$FRESH_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='audit_logs' AND name LIKE 'idx_audit%' ORDER BY name;" | tr '\n' ' ' | sed 's/ $//')"
[ "$ACTUAL_INDEXES" = "$EXPECTED_INDEXES" ] || fail "T-26: expected indexes [$EXPECTED_INDEXES], found [$ACTUAL_INDEXES]"
echo "PASS T-26: all four audit_logs indexes exist after 0004"

# ── T-27 — an empty actor_id is rejected after 0004 (CHECK, not the dropped FK) ──
POST_0004_T27_DB="$WORKDIR/post-0004-t27.db"
cp "$FRESH_DB" "$POST_0004_T27_DB"
if sqlite3 "$POST_0004_T27_DB" "INSERT INTO audit_logs (id, actor_id, resource_type, action_type) VALUES ('t27-row', '', 'target', 'create');" 2>/dev/null; then
  fail "T-27: an empty actor_id was accepted after 0004 dropped the foreign key"
fi
echo "PASS T-27: an empty actor_id is rejected after 0004 (CHECK constraint)"

# ── T-25 / T-29c — every classification-relevant column, including
#    prev_hash/row_hash, is preserved byte-for-byte across 0004.
#    Classification (crates/core/src/db/audit.rs::walk_chain) is a
#    pure function of exactly these column values (subject 05) -- if
#    they are byte-identical before and after, the classification
#    verify_chain would produce is provably identical too, without
#    needing to invoke Rust from this script. ──
SNAPSHOT_QUERY="SELECT id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, previous_value, new_value, result, ip_address, prev_hash, row_hash FROM audit_logs ORDER BY id;"

T25_DB="$WORKDIR/t25.db"
cp "$PRE_0004_DB" "$T25_DB"
sqlite3 "$T25_DB" <<SQL || fail "T-25: seeding fixture rows failed"
INSERT INTO audit_logs (id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, previous_value, new_value, result, ip_address, prev_hash, row_hash) VALUES
  ('row-1', '2026-01-01T00:00:00Z', 'u-admin', 'admin@example.com', 'target', 't-1', 'create', NULL, '{"name":"API"}', 'success', '203.0.113.5', '$GENESIS', 'hash1aaaa'),
  ('row-2', '2026-01-01T00:00:01Z', 'u-admin', 'admin@example.com', 'target', 't-1', 'update', '{"name":"API"}', '{"name":"API2"}', 'success', NULL, 'hash1aaaa', 'hash2bbbb'),
  ('row-legacy', '2020-01-01T00:00:00Z', 'u-old', NULL, 'user', 'u-old', 'login', NULL, NULL, 'success', NULL, NULL, NULL);
SQL
BEFORE="$(sqlite3 "$T25_DB" "$SNAPSHOT_QUERY")"
apply_sql_file "$T25_DB" "$SQL_DIR/0004_audit_actor_snapshot.sql" "$WORKDIR/t25-0004.err" || fail "T-25: 0004 failed to apply to the seeded fixture: $(cat "$WORKDIR/t25-0004.err")"
AFTER="$(sqlite3 "$T25_DB" "$SNAPSHOT_QUERY")"
[ "$BEFORE" = "$AFTER" ] || fail "T-25: classification-relevant columns changed across 0004
before:
$BEFORE
after:
$AFTER"
echo "PASS T-25: every classification-relevant column is preserved byte-for-byte across 0004"

HASHES_BEFORE="$(echo "$BEFORE" | cut -d'|' -f1,12,13)"
HASHES_AFTER="$(echo "$AFTER" | cut -d'|' -f1,12,13)"
[ "$HASHES_BEFORE" = "$HASHES_AFTER" ] || fail "T-29c: prev_hash/row_hash changed across 0004"
echo "PASS T-29c: prev_hash/row_hash preserved byte-for-byte across 0004"

# ── T-28 — deactivating or renaming a user alters no historical audit row ──
T28_DB="$WORKDIR/t28.db"
cp "$FRESH_DB" "$T28_DB"
sqlite3 "$T28_DB" <<SQL || fail "T-28: seeding fixture failed"
INSERT INTO users (id, email, name, role) VALUES ('u-1', 'original@example.com', 'Original Name', 'admin');
INSERT INTO audit_logs (id, actor_id, actor_email, resource_type, action_type) VALUES ('t28-row', 'u-1', 'original@example.com', 'target', 'create');
SQL
sqlite3 "$T28_DB" "UPDATE users SET email = 'renamed@example.com', name = 'Renamed', is_active = 0 WHERE id = 'u-1';" || fail "T-28: deactivating/renaming the user failed"
STORED_EMAIL="$(sqlite3 "$T28_DB" "SELECT actor_email FROM audit_logs WHERE id = 't28-row';")"
[ "$STORED_EMAIL" = "original@example.com" ] || fail "T-28: expected the historical audit row's actor_email to stay 'original@example.com', found '$STORED_EMAIL'"
echo "PASS T-28: deactivating/renaming a user alters no historical audit row"

# ── T-29a — 0004 refuses to apply to a Class A fixture, and leaves it untouched ──
#
# apply_sql_file's transaction wrap is what makes the "untouched" half
# of this test real: plain `sqlite3 file < script.sql` prints the
# parse error on the failing statement and keeps executing the rest of
# the script, so DROP/RENAME/CREATE INDEX still ran and this test
# would silently pass on a false premise -- confirmed by hand (the
# migration completed against a Class A fixture under plain sqlite3).
T29A_SQL="$WORKDIR/0.1.0-0001_initial.sql"
git -C "$REPO_ROOT" show 0.1.0:sql/0001_initial.sql > "$T29A_SQL" 2>/dev/null || fail "T-29a: could not read sql/0001_initial.sql as it existed at tag 0.1.0"
T29A_DB="$WORKDIR/t29a-classA.db"
apply_sql_file "$T29A_DB" "$T29A_SQL" "$WORKDIR/t29a-setup.err" || fail "T-29a: the 0.1.0-vintage schema itself failed to apply: $(cat "$WORKDIR/t29a-setup.err")"
BEFORE_SCHEMA="$(sqlite3 "$T29A_DB" ".schema audit_logs")"
if apply_sql_file "$T29A_DB" "$SQL_DIR/0004_audit_actor_snapshot.sql" "$WORKDIR/t29a.err"; then
  fail "T-29a: 0004 was expected to refuse a Class A fixture, but it applied"
fi
grep -q "no such column: prev_hash" "$WORKDIR/t29a.err" || fail "T-29a: expected 'no such column: prev_hash', got: $(cat "$WORKDIR/t29a.err")"
AFTER_SCHEMA="$(sqlite3 "$T29A_DB" ".schema audit_logs")"
[ "$BEFORE_SCHEMA" = "$AFTER_SCHEMA" ] || fail "T-29a: audit_logs's schema changed despite 0004 failing to apply"
LEFTOVER="$(sqlite3 "$T29A_DB" "SELECT count(*) FROM sqlite_master WHERE name='audit_logs_new';")"
[ "$LEFTOVER" -eq 0 ] || fail "T-29a: audit_logs_new was left behind despite 0004 failing to apply"
echo "PASS T-29a: 0004 refuses a Class A fixture (no such column: prev_hash) and leaves it untouched"

# ── T-03 — a deliberately broken migration fails the gate ──
BROKEN_DIR="$WORKDIR/broken-sql"
mkdir -p "$BROKEN_DIR"
cp "$SQL_DIR"/*.sql "$BROKEN_DIR"/
echo "THIS IS NOT VALID SQL;" > "$BROKEN_DIR/9999_deliberately_broken.sql"
BROKEN_DB="$WORKDIR/broken.db"
GATE_FAILED=0
for f in $(find "$BROKEN_DIR" -maxdepth 1 -name '*.sql' | sort); do
  apply_sql_file "$BROKEN_DB" "$f" "$WORKDIR/broken.err" || { GATE_FAILED=1; break; }
done
[ "$GATE_FAILED" -eq 1 ] || fail "T-03: a deliberately broken migration did not fail the gate"
echo "PASS T-03: a deliberately broken migration fails the gate"

# ── T-01a — a Class A database (tag 0.1.0, no hash columns) is reported,
#            not silently treated as healthy ──
CLASS_A_SQL="$WORKDIR/0.1.0-0001_initial.sql"
if ! git -C "$REPO_ROOT" show 0.1.0:sql/0001_initial.sql > "$CLASS_A_SQL" 2>/dev/null; then
  fail "T-01a: could not read sql/0001_initial.sql as it existed at tag 0.1.0"
fi
CLASS_A_DB="$WORKDIR/classA.db"
apply_sql_file "$CLASS_A_DB" "$CLASS_A_SQL" "$WORKDIR/classA.err" || fail "T-01a: the 0.1.0-vintage schema itself failed to apply: $(cat "$WORKDIR/classA.err")"
if sqlite3 "$CLASS_A_DB" "SELECT prev_hash, row_hash FROM audit_logs LIMIT 1;" 2>/dev/null; then
  fail "T-01a: expected the Class A fixture to lack prev_hash/row_hash, but the query succeeded"
fi
echo "PASS T-01a: a Class A database is detected as lacking prev_hash/row_hash"
echo "  (the Core-side request-time assertion in crates/core/src/db/audit.rs"
echo "  reproduces this same query; see assert_hash_columns_present)"

# ── T-76/T-77/T-78 (subject 15, G-11): at most one open incident per
#    target, enforced by the database. Per pre-flight
#    .git-exclude/reviewed/063-m2c-preflight.md §6, this is the right
#    instrument -- "the database refuses it" needs a fresh sqlite3
#    database, no D1 or Wrangler (G-37 is still open: noye-core cannot
#    run wasm tests at all, so there is nowhere else to put these). ──
PRE_0007_DB="$WORKDIR/pre-0007.db"
apply_sql_file "$PRE_0007_DB" "$SQL_DIR/0001_initial.sql" "$WORKDIR/setup15-0001.err" || fail "setup: 0001 failed for the pre-0007 fixture: $(cat "$WORKDIR/setup15-0001.err")"
apply_sql_file "$PRE_0007_DB" "$SQL_DIR/0003_audit_retention_exemption.sql" "$WORKDIR/setup15-0003.err" || fail "setup: 0003 failed for the pre-0007 fixture: $(cat "$WORKDIR/setup15-0003.err")"
apply_sql_file "$PRE_0007_DB" "$SQL_DIR/0004_audit_actor_snapshot.sql" "$WORKDIR/setup15-0004.err" || fail "setup: 0004 failed for the pre-0007 fixture: $(cat "$WORKDIR/setup15-0004.err")"
apply_sql_file "$PRE_0007_DB" "$SQL_DIR/0005_target_thresholds.sql" "$WORKDIR/setup15-0005.err" || fail "setup: 0005 failed for the pre-0007 fixture: $(cat "$WORKDIR/setup15-0005.err")"
apply_sql_file "$PRE_0007_DB" "$SQL_DIR/0006_suppression_scope_and_flags.sql" "$WORKDIR/setup15-0006.err" || fail "setup: 0006 failed for the pre-0007 fixture: $(cat "$WORKDIR/setup15-0006.err")"

sqlite3 "$PRE_0007_DB" <<SQL || fail "T-76/T-78: seeding the pre-0007 fixture failed"
INSERT INTO users (id, email, name, role) VALUES ('u-t76', 't76@example.com', 'T76', 'admin');
INSERT INTO targets (id, name, type, host, owner_id, created_by, updated_by) VALUES ('tgt-t76', 't76', 'https', 't76.example.com', 'u-t76', 'u-t76', 'u-t76');
INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('inc-t76-1', 'tgt-t76', 'open', '2026-01-01T00:00:00Z', 'first');
SQL
if ! sqlite3 "$PRE_0007_DB" "INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('inc-t76-2', 'tgt-t76', 'open', '2026-01-01T01:00:00Z', 'second');" 2>/dev/null; then
  fail "T-76 baseline: expected a second open incident for the same target to succeed before 0007, but it failed"
fi
echo "PASS T-76 baseline (must-fail-first): pre-0007, a second open incident for the same target is accepted"

POST_0007_DB="$WORKDIR/post-0007.db"
cp "$PRE_0007_DB" "$POST_0007_DB"
apply_sql_file "$POST_0007_DB" "$SQL_DIR/0007_incident_one_open_index.sql" "$WORKDIR/t76-0007.err" || fail "T-76: 0007 failed to apply: $(cat "$WORKDIR/t76-0007.err")"

# T-78 — the pre-existing duplicate (inc-t76-1/inc-t76-2, both 'open'
# before 0007 ran) is resolved by the migration itself: the earlier one
# stays open, the later one is force-resolved and says so.
OPEN_COUNT_T78="$(sqlite3 "$POST_0007_DB" "SELECT count(*) FROM incidents WHERE target_id='tgt-t76' AND status='open';")"
[ "$OPEN_COUNT_T78" -eq 1 ] || fail "T-78: expected exactly one open incident for tgt-t76 after 0007 resolved the duplicate, found $OPEN_COUNT_T78"
STILL_OPEN_T78="$(sqlite3 "$POST_0007_DB" "SELECT id FROM incidents WHERE target_id='tgt-t76' AND status='open';")"
[ "$STILL_OPEN_T78" = "inc-t76-1" ] || fail "T-78: expected the earlier incident (inc-t76-1) to remain open, found '$STILL_OPEN_T78'"
RESOLVED_NOTE_T78="$(sqlite3 "$POST_0007_DB" "SELECT resolution_note FROM incidents WHERE id='inc-t76-2';")"
case "$RESOLVED_NOTE_T78" in
  *duplicate*) : ;;
  *) fail "T-78: expected inc-t76-2's resolution_note to explain the auto-resolve, got '$RESOLVED_NOTE_T78'" ;;
esac
echo "PASS T-78: pre-existing duplicates are resolved by the migration (earliest kept open, later one auto-resolved and reported: '$RESOLVED_NOTE_T78')"

# T-76 — bypasses the API and inserts directly, per the handoff's own
# instruction: the point is that the *database* refuses it, not that
# application flow control still works (which was never in doubt).
if sqlite3 "$POST_0007_DB" "INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('inc-t76-3', 'tgt-t76', 'open', '2026-01-01T02:00:00Z', 'third');" 2>/dev/null; then
  fail "T-76: a second open incident for the same target was accepted after 0007"
fi
echo "PASS T-76: after 0007, a second open incident for the same target is refused by the database"

# T-77 — resolving the still-open one frees the target up for a new one.
sqlite3 "$POST_0007_DB" "UPDATE incidents SET status = 'resolved', resolved_at = '2026-01-01T03:00:00Z' WHERE id = 'inc-t76-1';" || fail "T-77: resolving inc-t76-1 failed"
if ! sqlite3 "$POST_0007_DB" "INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('inc-t76-4', 'tgt-t76', 'open', '2026-01-01T04:00:00Z', 'fourth');" 2>"$WORKDIR/t77.err"; then
  cat "$WORKDIR/t77.err" >&2
  fail "T-77: a new open incident was refused even after the prior one resolved"
fi
echo "PASS T-77: resolving the open incident allows a new one for the same target"

echo
echo "All migration gate checks passed."
