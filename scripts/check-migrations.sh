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

# expect_rejected <db> <label> <sql> -- fails the gate if <sql> succeeds
expect_rejected() {
  local db="$1" label="$2" sql="$3"
  if sqlite3 "$db" "$sql" 2>/dev/null; then
    fail "$label: expected the database to refuse this, but it succeeded"
  fi
}

# expect_accepted <db> <label> <sql> -- fails the gate if <sql> fails
expect_accepted() {
  local db="$1" label="$2" sql="$3"
  local err
  err="$(sqlite3 "$db" "$sql" 2>&1)" || fail "$label: expected the database to accept this, but it was refused -- $err"
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

# ── T-26 — the four original audit_logs indexes from 0004 are still
#    present in the fully-migrated schema. $FRESH_DB has every
#    migration applied, not just 0004 -- subject 18 (0009) added a
#    fifth, idx_audit_action_type (G-15), which belongs in the
#    expected set now for the same reason a fifth index existing at
#    all is correct: T-26 asserts these indexes survive, not that
#    nothing is ever added. ──
EXPECTED_INDEXES="idx_audit_action_type idx_audit_actor idx_audit_resource idx_audit_row_hash idx_audit_time"
ACTUAL_INDEXES="$(sqlite3 "$FRESH_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='audit_logs' AND name LIKE 'idx_audit%' ORDER BY name;" | tr '\n' ' ' | sed 's/ $//')"
[ "$ACTUAL_INDEXES" = "$EXPECTED_INDEXES" ] || fail "T-26: expected indexes [$EXPECTED_INDEXES], found [$ACTUAL_INDEXES]"
echo "PASS T-26: the four original audit_logs indexes (0004) plus idx_audit_action_type (0009) are all present"

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

# T-78a (ruling 064 §1, G-14): the migration's own resolved_at must be
# this application's RFC 3339 form ("YYYY-MM-DDTHH:MM:SSZ"), not
# SQLite's datetime('now') form ("YYYY-MM-DD HH:MM:SS" -- space, no
# 'Z'). db/incidents.rs's SLA/MTTR window filter (resolved_at > ?2) is
# a *string* comparison, and ' ' (0x20) sorts before 'T' (0x54): the
# space-separated form would silently fail that comparison and drop
# the row from every report window that should have included it.
RESOLVED_AT_T78A="$(sqlite3 "$POST_0007_DB" "SELECT resolved_at FROM incidents WHERE id='inc-t76-2';")"
case "$RESOLVED_AT_T78A" in
  *T*Z) : ;;
  *) fail "T-78a: expected inc-t76-2's resolved_at to be RFC 3339 (contain 'T', end in 'Z'), got '$RESOLVED_AT_T78A' -- this is G-14's shape, and the app's own window filter compares this column as a string" ;;
esac
echo "PASS T-78a: the migration's own resolved_at is RFC 3339 ('$RESOLVED_AT_T78A'), matching the application's format -- not SQLite's datetime('now') form"

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

# ── T-83/T-84/T-85 (subject 17, G-17/G-28, DEC-014) and T-87..T-94
#    (subject 18, G-13/G-14/G-15), all against migration 0009. Same
#    instrument as T-76..T-78: a fresh sqlite3 database, no D1, no
#    Wrangler -- every one of these is a schema refusal, an accepted
#    boundary value, or an index/constraint's existence, and G-37
#    means noye-core has nowhere else to put them. ──
PRE_0009_DB="$WORKDIR/pre-0009.db"
for f in 0001_initial.sql 0003_audit_retention_exemption.sql 0004_audit_actor_snapshot.sql \
         0005_target_thresholds.sql 0006_suppression_scope_and_flags.sql \
         0007_incident_one_open_index.sql 0008_incident_actor_columns.sql; do
  apply_sql_file "$PRE_0009_DB" "$SQL_DIR/$f" "$WORKDIR/setup18-$f.err" \
    || fail "setup: $f failed for the pre-0009 fixture: $(cat "$WORKDIR/setup18-$f.err")"
done

sqlite3 "$PRE_0009_DB" <<'SQL' || fail "setup: seeding the pre-0009 fixture failed"
INSERT INTO users (id, email, name, role) VALUES ('u1', 'u1@example.com', 'U1', 'admin');
INSERT INTO targets (id, name, type, host, port, expected_status, timeout_sec, retry_count,
                      interval_minutes, tls_threshold_days, owner_id, created_by, updated_by,
                      success_threshold, failure_threshold)
  VALUES ('t1', 't1', 'https', 't1.example.com', 443, 200, 10, 3, 5, 30, 'u1', 'u1', 'u1', 3, 3);
INSERT INTO target_states (target_id, current_status) VALUES ('t1', 'unknown');
INSERT INTO notification_channels (id, name, channel_type, endpoint, owner_id)
  VALUES ('c1', 'c1', 'webhook', 'https://example.com/hook', 'u1');
SQL

POST_0009_DB="$WORKDIR/post-0009.db"
cp "$PRE_0009_DB" "$POST_0009_DB"
apply_sql_file "$POST_0009_DB" "$SQL_DIR/0009_schema_integrity.sql" "$WORKDIR/0009.err" \
  || fail "0009 failed to apply to the seeded fixture: $(cat "$WORKDIR/0009.err")"

# ── T-83/T-84/T-85 — unreachable states rejected. Baselines run
#    against disposable copies of PRE_0009_DB, not the shared original
#    -- $PRE_0009_DB is reused later (the audit-chain guard), and an
#    `expect_accepted` call that succeeds permanently mutates whatever
#    database it's given. ──

T83_PRE="$WORKDIR/t83-pre.db"; cp "$PRE_0009_DB" "$T83_PRE"
expect_accepted "$T83_PRE" "T-83 baseline (must-fail-first)" \
  "INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('i-ack', 't1', 'acknowledged', '2026-01-01T00:00:00Z', 'x')"
expect_rejected "$POST_0009_DB" "T-83" \
  "INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('i-ack', 't1', 'acknowledged', '2026-01-01T00:00:00Z', 'x')"
echo "PASS T-83: incidents.status = 'acknowledged' is rejected after 0009 (accepted before)"

T84_PRE="$WORKDIR/t84-pre.db"; cp "$PRE_0009_DB" "$T84_PRE"
expect_accepted "$T84_PRE" "T-84 baseline (must-fail-first)" \
  "UPDATE target_states SET current_status = 'degraded' WHERE target_id = 't1'"
expect_rejected "$POST_0009_DB" "T-84" \
  "UPDATE target_states SET current_status = 'degraded' WHERE target_id = 't1'"
echo "PASS T-84: target_states.current_status = 'degraded' is rejected after 0009 (accepted before)"

T85_PRE="$WORKDIR/t85-pre.db"; cp "$PRE_0009_DB" "$T85_PRE"
expect_accepted "$T85_PRE" "T-85 baseline (must-fail-first)" \
  "UPDATE target_states SET current_status = 'maintenance' WHERE target_id = 't1'"
expect_rejected "$POST_0009_DB" "T-85" \
  "UPDATE target_states SET current_status = 'maintenance' WHERE target_id = 't1'"
echo "PASS T-85: target_states.current_status = 'maintenance' is rejected after 0009 (accepted before)"

# ── T-87 — every boolean column rejects a value other than 0 or 1.
#    Ten columns, seven tables (subject 18's own enumerated list --
#    not derived by searching, per its own warning about
#    target_states' counters). Each entry: a label, and a full INSERT/
#    UPDATE statement writing 2 into that one column. ──

T87_CASES=(
  "users.is_active|UPDATE users SET is_active = 2 WHERE id = 'u1'"
  "targets.is_disabled|UPDATE targets SET is_disabled = 2 WHERE id = 't1'"
  "check_results.is_success|INSERT INTO check_results (id, target_id, is_success) VALUES ('r-t87', 't1', 2)"
  "maintenance_windows.suppress_notify|INSERT INTO maintenance_windows (id, name, start_at, end_at, target_id, suppress_notify, created_by, updated_by) VALUES ('m-t87a', 'm', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 't1', 2, 'u1', 'u1')"
  "maintenance_windows.exclude_from_sla|INSERT INTO maintenance_windows (id, name, start_at, end_at, target_id, exclude_from_sla, created_by, updated_by) VALUES ('m-t87b', 'm', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 't1', 2, 'u1', 'u1')"
  "maintenance_windows.is_active|INSERT INTO maintenance_windows (id, name, start_at, end_at, target_id, is_active, created_by, updated_by) VALUES ('m-t87c', 'm', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 't1', 2, 'u1', 'u1')"
  "notification_channels.is_enabled|UPDATE notification_channels SET is_enabled = 2 WHERE id = 'c1'"
  "target_notifications.on_down|INSERT INTO target_notifications (target_id, channel_id, on_down) VALUES ('t1', 'c1', 2)"
  "target_notifications.on_up|INSERT INTO target_notifications (target_id, channel_id, on_up) VALUES ('t1', 'c1', 2)"
  "retention_policies.archive_to_r2|UPDATE retention_policies SET archive_to_r2 = 2 WHERE table_name = 'check_results'"
)
for case in "${T87_CASES[@]}"; do
  label="${case%%|*}"
  sql="${case#*|}"
  PRE_COPY="$WORKDIR/t87-pre-$(echo "$label" | tr '.' '-').db"
  cp "$PRE_0009_DB" "$PRE_COPY"
  expect_accepted "$PRE_COPY" "T-87 baseline (must-fail-first, $label)" "$sql"
  POST_COPY="$WORKDIR/t87-post-$(echo "$label" | tr '.' '-').db"
  cp "$POST_0009_DB" "$POST_COPY"
  expect_rejected "$POST_COPY" "T-87 ($label)" "$sql"
done
echo "PASS T-87: all ten boolean columns reject a value other than 0 or 1 after 0009 (accepted before)"

# ── T-88/T-92 — each numeric range rejects one value below and one
#    above its bound, and accepts the boundary values themselves.
#    Independent target rows per case so one failing UPDATE doesn't
#    disturb another case's fixture. ──

T88_RANGES=(
  "targets.port|port|0|65536|1|65535"
  "targets.expected_status|expected_status|99|600|100|599"
  "targets.timeout_sec|timeout_sec|0|301|1|300"
  "targets.retry_count|retry_count|-1|11|0|10"
  "targets.interval_minutes|interval_minutes|0|1441|1|1440"
)
for range in "${T88_RANGES[@]}"; do
  IFS='|' read -r label col below above lo hi <<<"$range"
  expect_rejected "$POST_0009_DB" "T-88 ($label, below: $below)" \
    "UPDATE targets SET $col = $below WHERE id = 't1'"
  expect_rejected "$POST_0009_DB" "T-88 ($label, above: $above)" \
    "UPDATE targets SET $col = $above WHERE id = 't1'"
  expect_accepted "$POST_0009_DB" "T-92 ($label, lower boundary: $lo)" \
    "UPDATE targets SET $col = $lo WHERE id = 't1'"
  expect_accepted "$POST_0009_DB" "T-92 ($label, upper boundary: $hi)" \
    "UPDATE targets SET $col = $hi WHERE id = 't1'"
  # Restore a mid-range value so later cases aren't testing against a
  # target row left at a boundary by the case before them.
  sqlite3 "$POST_0009_DB" "UPDATE targets SET port = 443, expected_status = 200, timeout_sec = 10, retry_count = 3, interval_minutes = 5 WHERE id = 't1';" \
    || fail "T-88: could not restore t1's mid-range values after $label"
done
expect_rejected "$POST_0009_DB" "T-88 (targets.tls_threshold_days, below: -1)" \
  "UPDATE targets SET tls_threshold_days = -1 WHERE id = 't1'"
expect_accepted "$POST_0009_DB" "T-92 (targets.tls_threshold_days, lower boundary: 0)" \
  "UPDATE targets SET tls_threshold_days = 0 WHERE id = 't1'"
echo "PASS T-88: every numeric range rejects one value below and one above its bound"
echo "PASS T-92: valid values at each boundary are accepted"

# ── T-89 — thresholds reject 0 and reject 11 (zero must not be
#    representable -- it would mean "transition on no evidence") ──

expect_rejected "$POST_0009_DB" "T-89 (success_threshold = 0)" \
  "UPDATE targets SET success_threshold = 0 WHERE id = 't1'"
expect_rejected "$POST_0009_DB" "T-89 (success_threshold = 11)" \
  "UPDATE targets SET success_threshold = 11 WHERE id = 't1'"
expect_rejected "$POST_0009_DB" "T-89 (failure_threshold = 0)" \
  "UPDATE targets SET failure_threshold = 0 WHERE id = 't1'"
expect_rejected "$POST_0009_DB" "T-89 (failure_threshold = 11)" \
  "UPDATE targets SET failure_threshold = 11 WHERE id = 't1'"
echo "PASS T-89: success_threshold/failure_threshold reject 0 and reject 11"

# ── T-90 — a window with end_at <= start_at is rejected by the database ──

T90_PRE="$WORKDIR/t90-pre.db"; cp "$PRE_0009_DB" "$T90_PRE"
expect_accepted "$T90_PRE" "T-90 baseline (must-fail-first)" \
  "INSERT INTO maintenance_windows (id, name, start_at, end_at, created_by, updated_by) VALUES ('m-t90', 'm', '2026-01-01T12:00:00Z', '2026-01-01T12:00:00Z', 'u1', 'u1')"
expect_rejected "$POST_0009_DB" "T-90 (end_at == start_at)" \
  "INSERT INTO maintenance_windows (id, name, start_at, end_at, created_by, updated_by) VALUES ('m-t90a', 'm', '2026-01-01T12:00:00Z', '2026-01-01T12:00:00Z', 'u1', 'u1')"
expect_rejected "$POST_0009_DB" "T-90 (end_at < start_at)" \
  "INSERT INTO maintenance_windows (id, name, start_at, end_at, created_by, updated_by) VALUES ('m-t90b', 'm', '2026-01-01T12:00:00Z', '2026-01-01T11:00:00Z', 'u1', 'u1')"
expect_accepted "$POST_0009_DB" "T-90 guard (end_at > start_at)" \
  "INSERT INTO maintenance_windows (id, name, start_at, end_at, created_by, updated_by) VALUES ('m-t90c', 'm', '2026-01-01T12:00:00Z', '2026-01-01T13:00:00Z', 'u1', 'u1')"
echo "PASS T-90: a window with end_at <= start_at is rejected by the database after 0009 (accepted before)"

# ── T-91 — a row written by schema default and one written by the
#    application, for the SAME instant, sort identically. Rather than
#    firing a live INSERT and hoping two statements land in the same
#    wall-clock second (real, if small, flakiness), this drives the
#    exact transform each source applies -- datetime('now') pre-0009,
#    strftime('%Y-%m-%dT%H:%M:%SZ','now') post-0009 and always in the
#    app -- against one fixed literal instant. Same function, same
#    input, deterministic: this is what the DEFAULT clause actually
#    does differently, without depending on real time at all. Do not
#    assert on the string's format -- assert on the values the
#    scheduler's own comparison would receive being equal. ──

SAME_INSTANT="2026-06-01 23:59:59"
PRE_0009_DEFAULT_STYLE="$(sqlite3 "$PRE_0009_DB" "SELECT datetime('$SAME_INSTANT');")"
APP_STYLE="$(sqlite3 "$POST_0009_DB" "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', '$SAME_INSTANT');")"
[ "$PRE_0009_DEFAULT_STYLE" != "$APP_STYLE" ] \
  || fail "T-91 baseline (must-fail-first): expected the pre-0009 schema-default style ('$PRE_0009_DEFAULT_STYLE') to differ from the application's own style ('$APP_STYLE') for the same instant, but they matched"
echo "PASS T-91 baseline (must-fail-first): pre-0009, the schema-default style ('$PRE_0009_DEFAULT_STYLE') and the application's style ('$APP_STYLE') disagree for the same instant"

POST_0009_DEFAULT_STYLE="$(sqlite3 "$POST_0009_DB" "SELECT strftime('%Y-%m-%dT%H:%M:%SZ', '$SAME_INSTANT');")"
[ "$POST_0009_DEFAULT_STYLE" = "$APP_STYLE" ] \
  || fail "T-91: expected the post-0009 schema-default style ('$POST_0009_DEFAULT_STYLE') to match the application's style ('$APP_STYLE') for the same instant"
echo "PASS T-91: after 0009, a row written by schema default and one written by the application sort identically for the same instant"

# ── T-93 — every listed access path is index-supported ──

EXPECTED_IDX_CHANNELS_OWNER="idx_channels_owner"
ACTUAL_IDX_CHANNELS_OWNER="$(sqlite3 "$POST_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='notification_channels' AND name='idx_channels_owner';")"
[ "$ACTUAL_IDX_CHANNELS_OWNER" = "$EXPECTED_IDX_CHANNELS_OWNER" ] || fail "T-93: idx_channels_owner (notification_channels.owner_id) is missing"

EXPECTED_IDX_TN_CHANNEL="idx_target_notifications_channel"
ACTUAL_IDX_TN_CHANNEL="$(sqlite3 "$POST_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='target_notifications' AND name='idx_target_notifications_channel';")"
[ "$ACTUAL_IDX_TN_CHANNEL" = "$EXPECTED_IDX_TN_CHANNEL" ] || fail "T-93: idx_target_notifications_channel (target_notifications.channel_id, the reverse channel-to-target lookup) is missing"

ACTUAL_IDX_AUDIT_ACTION="$(sqlite3 "$POST_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='audit_logs' AND name='idx_audit_action_type';")"
[ "$ACTUAL_IDX_AUDIT_ACTION" = "idx_audit_action_type" ] || fail "T-93: idx_audit_action_type (audit_logs.action_type filtering) is missing"

ACTUAL_IDX_INCIDENTS_TARGET="$(sqlite3 "$POST_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='incidents' AND name='idx_incidents_target';")"
[ "$ACTUAL_IDX_INCIDENTS_TARGET" = "idx_incidents_target" ] || fail "T-93: idx_incidents_target (incident ordering) is missing"

ACTUAL_IDX_MAINT_ACTIVE="$(sqlite3 "$POST_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='maintenance_windows' AND name='idx_maint_active';")"
[ "$ACTUAL_IDX_MAINT_ACTIVE" = "idx_maint_active" ] || fail "T-93: idx_maint_active (window overlap) is missing"

echo "PASS T-93: every listed access path (channel owner, channel-to-target, audit action_type, incident ordering, window overlap) is index-supported"

# ── T-94 — every constraint and index present after 0008 is still
#    present after 0009, except the two DEC-014 removes ('acknowledged'
#    from incidents.status, 'degraded'/'maintenance' from target_
#    states.current_status) and incidents.created_by, which 0009 is
#    explicitly the migration that drops (ruling 064 §4.1). Enumerated
#    from sqlite_master, not from memory, per the handoff's own
#    instruction. ──

INDEXES_BEFORE="$(sqlite3 "$PRE_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name;")"
INDEXES_AFTER="$(sqlite3 "$POST_0009_DB" "SELECT name FROM sqlite_master WHERE type='index' AND name NOT LIKE 'sqlite_%' ORDER BY name;")"
for idx in $INDEXES_BEFORE; do
  echo "$INDEXES_AFTER" | grep -qx "$idx" || fail "T-94: index '$idx' existed after 0008 but is missing after 0009"
done
echo "PASS T-94 (indexes): every index present after 0008 is still present after 0009"

# maintenance_windows' scope-exclusivity CHECK (subject 12, G-08) must
# survive -- dropping it silently reopens G-08 in the milestone after
# it closed. Proved by behaviour, not by reading the schema text: the
# same insert that violated it before 0006 must still be refused.
expect_rejected "$POST_0009_DB" "T-94 (maintenance_windows scope-exclusivity CHECK survives)" \
  "INSERT INTO maintenance_windows (id, name, start_at, end_at, target_id, target_tag, created_by, updated_by) VALUES ('m-t94', 'm', '2026-01-01T00:00:00Z', '2026-01-01T01:00:00Z', 't1', 'sometag', 'u1', 'u1')"

# targets' shape from 0006 (no tags column) and 0005 (threshold
# columns) must survive.
TARGETS_COLS_AFTER="$(sqlite3 "$POST_0009_DB" "PRAGMA table_info(targets);" | cut -d'|' -f2)"
echo "$TARGETS_COLS_AFTER" | grep -qx "tags" && fail "T-94: targets.tags reappeared after 0009 -- subject 12's drop was reversed"
echo "$TARGETS_COLS_AFTER" | grep -qx "success_threshold" || fail "T-94: targets.success_threshold is missing after 0009"
echo "$TARGETS_COLS_AFTER" | grep -qx "failure_threshold" || fail "T-94: targets.failure_threshold is missing after 0009"

# incidents' opened_by/resolved_by (subject 16) must survive; created_by
# must NOT (this is the migration that drops it).
INCIDENTS_COLS_AFTER="$(sqlite3 "$POST_0009_DB" "PRAGMA table_info(incidents);" | cut -d'|' -f2)"
echo "$INCIDENTS_COLS_AFTER" | grep -qx "opened_by" || fail "T-94: incidents.opened_by is missing after 0009"
echo "$INCIDENTS_COLS_AFTER" | grep -qx "resolved_by" || fail "T-94: incidents.resolved_by is missing after 0009"
echo "$INCIDENTS_COLS_AFTER" | grep -qx "created_by" && fail "T-94: incidents.created_by is still present after 0009 -- it should have been dropped (ruling 064 §4.1)"

echo "PASS T-94 (columns): maintenance_windows' scope-exclusivity CHECK, targets' 0005/0006 shape, and incidents' opened_by/resolved_by all survive 0009; incidents.created_by does not"

# ── Guard: the audit hash chain survives 0009 unchanged. action_time
#    is copied byte-for-byte (see 0009's own header comment for why --
#    row_hash covers it), so every classification-relevant column,
#    including action_time itself, must be identical before and after,
#    the same shape as T-25/T-29c across 0004. ──

AUDIT_SNAPSHOT_QUERY="SELECT id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, previous_value, new_value, result, ip_address, prev_hash, row_hash FROM audit_logs ORDER BY id;"
sqlite3 "$PRE_0009_DB" <<SQL || fail "T-94 (audit guard): seeding an audit fixture row failed"
INSERT INTO audit_logs (id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, result, prev_hash, row_hash)
VALUES ('a-t94', '2026-01-01T00:00:00Z', 'u1', 'u1@example.com', 'target', 't1', 'create', 'success', '$GENESIS', 'hash-a-t94');
SQL
AUDIT_BEFORE="$(sqlite3 "$PRE_0009_DB" "$AUDIT_SNAPSHOT_QUERY")"
AUDIT_POST_DB="$WORKDIR/post-0009-audit.db"
cp "$PRE_0009_DB" "$AUDIT_POST_DB"
apply_sql_file "$AUDIT_POST_DB" "$SQL_DIR/0009_schema_integrity.sql" "$WORKDIR/0009-audit.err" \
  || fail "T-94 (audit guard): 0009 failed to apply to the audit-seeded fixture: $(cat "$WORKDIR/0009-audit.err")"
AUDIT_AFTER="$(sqlite3 "$AUDIT_POST_DB" "$AUDIT_SNAPSHOT_QUERY")"
[ "$AUDIT_BEFORE" = "$AUDIT_AFTER" ] || fail "T-94 (audit guard): a classification-relevant audit_logs column changed across 0009
before:
$AUDIT_BEFORE
after:
$AUDIT_AFTER"
echo "PASS T-94 (audit guard): every classification-relevant audit_logs column, including action_time itself, is preserved byte-for-byte across 0009"

echo
echo "All migration gate checks passed."
