#!/usr/bin/env bash
# scripts/classify-audit-schema.sh gate (subject: rfcs/handoffs/06a-classify-audit-schema.md).
#
# classify-audit-schema.sh's verdict decides whether subject 06's
# migration 0004 can be an ordinary static SQL file or must become a
# schema-introspecting routine. A wrong CLASS_A verdict re-scopes that
# work on the strength of a database that isn't actually Class A; a
# missed CLASS_A verdict lets a real Class A database roll back
# migration 0004 in production. Exercised here against disposable
# local D1 fixtures — same shape as scripts/check-migrations.sh and
# scripts/check-changelog-section.sh — instead of leaving it verified
# only in an evidence log against fixtures that no longer exist.
#
#   T-30a — Class A fixture (sql/0001_initial.sql as it stood at 0.1.0) -> CLASS_A
#   T-30b — Class C fixture (current sql/0001_initial.sql)              -> CLASS_BC
#   T-30c — empty database (no audit_logs table at all)                 -> NO_TABLE, not CLASS_A
#   T-30d — audit_logs with exactly one hash column                     -> MALFORMED
#   T-30e — the printed verdict matches the printed evidence on every fixture above
#
# T-30c is the one that matters most (per the handoff): a false CLASS_A
# finding would re-scope subject 06 on the strength of an empty
# database, not a real one.
#
# Uses `wrangler d1 execute --local` / `wrangler d1 migrations apply
# --local` only. Never touches a remote database.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CLASSIFIER="$REPO_ROOT/scripts/classify-audit-schema.sh"
WORKDIR="$(mktemp -d)"
trap 'rm -rf "$WORKDIR"' EXIT

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# make_fixture <name> <db-name> -- writes a wrangler.toml + empty
# migrations/ dir for <db-name> under $WORKDIR/<name>, and cds there.
make_fixture() {
  local dir="$WORKDIR/$1"
  local db_name="$2"
  mkdir -p "$dir/migrations"
  cat >"$dir/wrangler.toml" <<TOML
name = "check-classify-audit-schema-$1"
main = "index.js"
compatibility_date = "2024-04-13"

[[d1_databases]]
binding = "DB"
database_name = "$db_name"
database_id = "00000000-0000-0000-0000-000000000000"
TOML
  echo "export default { fetch() { return new Response('ok') } };" >"$dir/index.js"
}

apply_migrations() {
  local dir="$1"
  local db_name="$2"
  (cd "$dir" && wrangler d1 migrations apply "$db_name" --local >/dev/null 2>"$dir/apply.err") \
    || { cat "$dir/apply.err" >&2; fail "$3: migrations failed to apply"; }
}

classify() {
  local dir="$1"
  local db_name="$2"
  (cd "$dir" && bash "$CLASSIFIER" "$db_name" 2>"$dir/classify.err") \
    || { cat "$dir/classify.err" >&2; fail "$3: classifier itself exited non-zero"; }
}

# ── T-30a — Class A fixture: audit_logs with neither hash column ──
make_fixture "t30a" "t30a_db"
if ! git -C "$REPO_ROOT" show 0.1.0:sql/0001_initial.sql >"$WORKDIR/t30a/migrations/0001_initial.sql" 2>/dev/null; then
  fail "T-30a: could not read sql/0001_initial.sql as it existed at tag 0.1.0"
fi
apply_migrations "$WORKDIR/t30a" "t30a_db" "T-30a"
OUT_A="$(classify "$WORKDIR/t30a" "t30a_db" "T-30a")"
echo "$OUT_A" | grep -q "^CLASS_A$" || fail "T-30a: expected CLASS_A, got: $OUT_A"
echo "$OUT_A" | grep -q "^evidence: has_table=1 hash_cols=0$" || fail "T-30a: unexpected evidence line: $OUT_A"
echo "PASS T-30a: a Class A fixture (tag 0.1.0's sql/0001_initial.sql) classifies as CLASS_A"

# ── T-30b — Class C fixture: audit_logs with both hash columns ──
make_fixture "t30b" "t30b_db"
cp "$REPO_ROOT/sql/0001_initial.sql" "$WORKDIR/t30b/migrations/0001_initial.sql"
apply_migrations "$WORKDIR/t30b" "t30b_db" "T-30b"
OUT_B="$(classify "$WORKDIR/t30b" "t30b_db" "T-30b")"
echo "$OUT_B" | grep -q "^CLASS_BC$" || fail "T-30b: expected CLASS_BC, got: $OUT_B"
echo "$OUT_B" | grep -q "^evidence: has_table=1 hash_cols=2$" || fail "T-30b: unexpected evidence line: $OUT_B"
echo "PASS T-30b: the current sql/0001_initial.sql classifies as CLASS_BC"

# ── T-30c — empty database: no audit_logs table at all ──
# No migrations applied. T-30c is the one that matters: pragma_table_info
# on a non-existent table returns zero rows, so hash_cols alone would
# report 0 -- indistinguishable from Class A -- for an empty database
# too. has_table is what tells them apart.
make_fixture "t30c" "t30c_db"
OUT_C="$(classify "$WORKDIR/t30c" "t30c_db" "T-30c")"
echo "$OUT_C" | grep -q "^NO_TABLE$" || fail "T-30c: expected NO_TABLE, got: $OUT_C"
echo "$OUT_C" | grep -q "^CLASS_A$" && fail "T-30c: an empty database must NEVER classify as CLASS_A"
echo "$OUT_C" | grep -q "^evidence: has_table=0 hash_cols=0$" || fail "T-30c: unexpected evidence line: $OUT_C"
echo "PASS T-30c: an empty database classifies as NO_TABLE, never CLASS_A"

# ── T-30d — malformed: audit_logs with exactly one hash column ──
make_fixture "t30d" "t30d_db"
cat >"$WORKDIR/t30d/migrations/0001_initial.sql" <<'SQL'
CREATE TABLE audit_logs (
  id TEXT PRIMARY KEY,
  actor_id TEXT NOT NULL,
  prev_hash TEXT
);
SQL
apply_migrations "$WORKDIR/t30d" "t30d_db" "T-30d"
OUT_D="$(classify "$WORKDIR/t30d" "t30d_db" "T-30d")"
echo "$OUT_D" | grep -q "^MALFORMED$" || fail "T-30d: expected MALFORMED, got: $OUT_D"
echo "$OUT_D" | grep -q "^evidence: has_table=1 hash_cols=1$" || fail "T-30d: unexpected evidence line: $OUT_D"
echo "PASS T-30d: audit_logs with exactly one hash column classifies as MALFORMED"

# ── T-30e — the verdict never drifts from the evidence it printed ──
# Independently re-derive the expected verdict from each fixture's
# printed evidence line, rather than trusting the script's own
# if/elif chain -- guards against a future edit changing the mapping
# without changing what the evidence line reports.
check_no_drift() {
  local label="$1" out="$2"
  local verdict has_table hash_cols expected
  verdict="$(echo "$out" | sed -n '1p')"
  has_table="$(echo "$out" | sed -n '3p' | sed 's/.*has_table=\([0-9]*\).*/\1/')"
  hash_cols="$(echo "$out" | sed -n '3p' | sed 's/.*hash_cols=\([0-9]*\).*/\1/')"
  case "${has_table}|${hash_cols}" in
    "0|0") expected="NO_TABLE" ;;
    "1|0") expected="CLASS_A" ;;
    "1|2") expected="CLASS_BC" ;;
    *) expected="MALFORMED" ;;
  esac
  [ "$verdict" = "$expected" ] || fail "T-30e ($label): verdict '$verdict' does not match evidence has_table=$has_table hash_cols=$hash_cols (expected $expected)"
}
check_no_drift "T-30a" "$OUT_A"
check_no_drift "T-30b" "$OUT_B"
check_no_drift "T-30c" "$OUT_C"
check_no_drift "T-30d" "$OUT_D"
echo "PASS T-30e: the printed verdict matches the printed evidence on every fixture"

echo
echo "All classify-audit-schema gate checks passed."
