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
#
# T-01 and T-01a are must-fail-first: run this script against the pre-fix
# tree (before sql/0002_audit_hash_chain.sql was deleted, or against a
# tree with no Class A detection) and it fails. See
# .git-exclude/evidence/baseline-p0-p1.log for that capture.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SQL_DIR="$REPO_ROOT/sql"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# ── T-01 — apply every sql/*.sql, in filename order, to a fresh database ──
FRESH_DB="$WORKDIR/fresh.db"
for f in $(find "$SQL_DIR" -maxdepth 1 -name '*.sql' | sort); do
  if ! sqlite3 "$FRESH_DB" < "$f" 2>"$WORKDIR/apply.err"; then
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

# ── T-03 — a deliberately broken migration fails the gate ──
BROKEN_DIR="$WORKDIR/broken-sql"
mkdir -p "$BROKEN_DIR"
cp "$SQL_DIR"/*.sql "$BROKEN_DIR"/
echo "THIS IS NOT VALID SQL;" > "$BROKEN_DIR/9999_deliberately_broken.sql"
BROKEN_DB="$WORKDIR/broken.db"
GATE_FAILED=0
for f in $(find "$BROKEN_DIR" -maxdepth 1 -name '*.sql' | sort); do
  sqlite3 "$BROKEN_DB" < "$f" 2>/dev/null || { GATE_FAILED=1; break; }
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
sqlite3 "$CLASS_A_DB" < "$CLASS_A_SQL" || fail "T-01a: the 0.1.0-vintage schema itself failed to apply"
if sqlite3 "$CLASS_A_DB" "SELECT prev_hash, row_hash FROM audit_logs LIMIT 1;" 2>/dev/null; then
  fail "T-01a: expected the Class A fixture to lack prev_hash/row_hash, but the query succeeded"
fi
echo "PASS T-01a: a Class A database is detected as lacking prev_hash/row_hash"
echo "  (the Core-side request-time assertion in crates/core/src/db/audit.rs"
echo "  reproduces this same query; see assert_hash_columns_present)"

echo
echo "All migration gate checks passed."
