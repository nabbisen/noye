#!/usr/bin/env bash
# Classify a D1 database's audit_logs schema shape (subject 06a,
# rfcs/handoffs/06a-classify-audit-schema.md). Read-only: never writes
# to the database, never runs DDL.
#
# Determines whether subject 06's migration 0004 can be an ordinary
# static SQL file (Class B/C — audit_logs already has prev_hash and
# row_hash) or would need a schema-introspecting routine instead
# (Class A — audit_logs exists without them, from sql/0001_initial.sql
# as it stood at tag 0.1.0). See rfcs/handoffs/06-audit-actor-snapshot.md
# and .git-exclude/review-request/020-... for why a single static
# migration file cannot serve both shapes.
#
# Usage: scripts/classify-audit-schema.sh <database-name> [--remote]
#   Defaults to --local. --remote is a deliberate, explicit opt-in —
#   this script never touches a remote database unless told to.
#
# Prints, on stdout:
#   1. The verdict: CLASS_A | CLASS_BC | NO_TABLE | MALFORMED
#   2. A human-readable line explaining it
#   3. The raw has_table|hash_cols evidence it decided from
set -euo pipefail

DB_NAME="${1:?Usage: $0 <database-name> [--remote]}"
LOCATION_FLAG="--local"
if [ "${2:-}" = "--remote" ]; then
  LOCATION_FLAG="--remote"
fi

# has_table is load-bearing: pragma_table_info on a non-existent table
# returns zero rows, so hash_cols alone would report 0 for an empty
# database too, and misclassify it as Class A -- manufacturing the
# very finding this script exists to test for (T-30c).
QUERY="SELECT \
  (SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='audit_logs') AS has_table, \
  (SELECT COUNT(*) FROM pragma_table_info('audit_logs') WHERE name IN ('prev_hash','row_hash')) AS hash_cols;"

RAW="$(wrangler d1 execute "$DB_NAME" "$LOCATION_FLAG" --json --command "$QUERY")"

HAS_TABLE="$(printf '%s' "$RAW" | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['results'][0]['has_table'])")"
HASH_COLS="$(printf '%s' "$RAW" | python3 -c "import json,sys; print(json.load(sys.stdin)[0]['results'][0]['hash_cols'])")"

if [ "$HAS_TABLE" -eq 0 ] && [ "$HASH_COLS" -eq 0 ]; then
  VERDICT="NO_TABLE"
  MESSAGE="no audit_logs table -- not a provisioned Noye database, not Class A"
elif [ "$HAS_TABLE" -eq 1 ] && [ "$HASH_COLS" -eq 0 ]; then
  VERDICT="CLASS_A"
  MESSAGE="audit_logs exists with NEITHER hash column -- Class A. Subject 06 is blocked on this database."
elif [ "$HAS_TABLE" -eq 1 ] && [ "$HASH_COLS" -eq 2 ]; then
  VERDICT="CLASS_BC"
  MESSAGE="audit_logs exists with both hash columns -- Class B or C. A static migration 0004 is fine."
else
  VERDICT="MALFORMED"
  MESSAGE="audit_logs exists with exactly one hash column (has_table=${HAS_TABLE} hash_cols=${HASH_COLS}) -- not a class this project has ever described."
fi

echo "$VERDICT"
echo "$MESSAGE"
echo "evidence: has_table=${HAS_TABLE} hash_cols=${HASH_COLS}"
