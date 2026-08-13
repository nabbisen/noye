#!/usr/bin/env bash
# D1-behaviour gate (subject: rfcs/handoffs/07f-d1-behaviour-ci-harness.md).
#
# Four subjects (07a, 07d, 07e, 08-10) built a `wrangler dev --local`
# harness, verified a behavioural claim by hand, and threw the harness
# away. None of it ran in CI -- M2a's central claims (a re-import no
# longer cascades, an unresolvable owner is refused before any write)
# lived only in evidence logs. The source-scan gates that DO run in CI
# guard the code's *shape* (no `INSERT OR REPLACE` remains), not its
# *behaviour* (the cascade does not fire). This script is the
# difference between that gap being closed on purpose and being closed
# by luck -- .git-exclude/reviewed/054-d1-ci-harness-proposal.md.
#
# ── Option B, and the boundary is a hard rule ──
#
# This script drives real HTTP routes and the real `--test-scheduled`
# trigger against a scratch local D1 (real workerd, via `wrangler dev
# --local` -- see docs/src/development.md "Node is not Workers").
# It adds NO route, NO feature flag, and NO `#[cfg]` to noye-core or
# noye-gateway. Option C (a feature-gated `/__test/` surface) was
# rejected: a test surface reaching a deployed Worker would be worse
# than G-21. If a behaviour cannot be reached through a route or the
# scheduled trigger, it is out of scope for this script -- see the
# note on (d) below, which is exactly that case.
#
# ── Assertions ──
#
#   (a) G-22 — re-importing an existing target with on_conflict=replace
#       preserves its check results, incidents and channel attachments
#       (db/migration.rs's upsert_target, converted from `INSERT OR
#       REPLACE` to an `ON CONFLICT(...) DO UPDATE SET` upsert)
#   (b) G-31 — an import naming an owner_id that resolves nowhere is
#       refused before any write, in both dry run and applied
#       (db::migration::find_unresolvable_owners)
#   (c) G-06 — an imported target gets a target_states row in the same
#       operation and is monitorable: a real scheduled tick
#       (`/cdn-cgi/handler/scheduled`) selects and probes it, and an
#       imported failure_threshold=1 produces `down` after exactly one
#       failed check
#
# (d), DR-LIF-06 (a retention pass deletes only what it archived), is
# NOT included here. `db::retention::run_cleanup`'s only caller
# (`monitor::engine::run_scheduled_checks`) gates it behind
# `chrono::Utc::now()`'s real wall-clock minute equalling "00" -- not
# the scheduled event's nominal time, which the code never reads. There
# is no route to it and no way to make it fire deterministically inside
# a CI-appropriate budget without either modifying the Worker (this
# subject's hard rule forbids it) or manipulating the process clock (not
# attempted: unverified against workerd in this environment, and a flaky
# clock trick would make this gate less trustworthy, not more -- the
# G-32/G-33 lesson). Escalated per the handoff's own "behaviour
# unreachable through a route or the scheduled trigger -> architect"
# rule, rather than ported anyway. Three assertions that run beat four
# where one is fragile -- the handoff's own words for this situation.
#
# ── Shape ──
#
# Same discipline as scripts/check-migrations.sh and
# scripts/check-classify-audit-schema.sh: self-contained, scratch
# state, `trap` teardown -- extended here with a live `wrangler dev`
# server, which those two never needed. The scratch D1's name is
# derived from this process's PID, never hardcoded (the
# scripts/deployment-verify/ lesson, where a hardcoded `noye_db` was a
# blocking finding). Nothing is left under `crates/core/` afterward --
# not a `.git-exclude/`, not a scratch `wrangler.toml`, not local D1
# persistence state (T-209, T-220).

set -u
# pipefail matters here specifically: `d1_count` is `d1_exec ... | jq
# ...`, and `d1_count` itself is always called as `X=$(d1_count ...)`.
# A `fail`/`exit` inside a function reached through a pipe inside a
# command substitution only ever terminates that subshell -- the
# assignment still succeeds, with an empty string, and a later
# `[ "$BEFORE" = "$AFTER" ]` on two empty strings would pass on
# nothing. `pipefail` makes the substitution itself report failure,
# which every call site below checks explicitly.
set -o pipefail
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CORE_DIR="$REPO_ROOT/crates/core"
WORKDIR="$(mktemp -d)"
DB_NAME="check_d1_behaviour_$$"
TOKEN="check-d1-behaviour-token-$$"
PORT=18930
BASE_URL="http://127.0.0.1:$PORT"
DEV_LOG="$WORKDIR/dev.log"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

cleanup() {
  local exit_code=$?
  pkill -f "wrangler dev --local --test-scheduled --port $PORT" >/dev/null 2>&1
  sleep 1
  rm -f "$CORE_DIR/wrangler.toml"
  rm -rf "$CORE_DIR/.wrangler"
  rm -rf "$WORKDIR"
  exit $exit_code
}
trap cleanup EXIT

# ── T-222 — fail loudly when a dependency is missing, never skip vacuously ──
command -v wrangler >/dev/null 2>&1 || fail "wrangler is not installed -- cannot run the D1-behaviour gate (not skipping: a harness that silently skips reports green for nothing checked)"
command -v cargo >/dev/null 2>&1 || fail "cargo is not installed"
command -v worker-build >/dev/null 2>&1 || fail "worker-build is not installed"
command -v jq >/dev/null 2>&1 || fail "jq is not installed -- needed to parse D1 query results"

# ── setup: scratch wrangler.toml, derived DB name, no dev-fallback reuse ──
cat > "$CORE_DIR/wrangler.toml" <<TOML
name = "check-d1-behaviour"
main = "build/worker/shim.mjs"
compatibility_date = "2024-04-13"
workers_dev = false

[build]
command = "true"

[[d1_databases]]
binding = "DB"
database_name = "$DB_NAME"
database_id = "00000000-0000-0000-0000-000000000000"
migrations_dir = "../../sql"

[[r2_buckets]]
binding = "LOG_BUCKET"
bucket_name = "check-d1-behaviour-logs"

[vars]
GATEWAY_SHARED_TOKEN = "$TOKEN"
DEFAULT_TIMEOUT_SEC = "10"
DEFAULT_RETRY_COUNT = "3"
DEFAULT_INTERVAL_MIN = "5"
DATA_RETENTION_DAYS = "90"
TOML

echo "== Building noye-core =="
(cd "$CORE_DIR" && worker-build --release) >"$WORKDIR/build.log" 2>&1 \
  || { cat "$WORKDIR/build.log" >&2; fail "worker-build --release failed"; }

echo "== Applying migrations to scratch D1 ($DB_NAME) =="
(cd "$CORE_DIR" && wrangler d1 migrations apply --local "$DB_NAME") >"$WORKDIR/migrate.log" 2>&1 \
  || { cat "$WORKDIR/migrate.log" >&2; fail "migrations failed to apply"; }

echo "== Starting wrangler dev --local --test-scheduled =="
(cd "$CORE_DIR" && wrangler dev --local --test-scheduled --port "$PORT" >"$DEV_LOG" 2>&1 &)

ready=0
for _ in $(seq 1 60); do
  if curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/healthz" --max-time 1 2>/dev/null | grep -q '^200$'; then
    ready=1
    break
  fi
  sleep 1
done
[ "$ready" = "1" ] || { cat "$DEV_LOG" >&2; fail "wrangler dev did not become ready within 60s"; }

# ── request/query helpers ──

# admin_req <method> <path> [json-body]
admin_req() {
  local method="$1" path="$2" body="${3:-}"
  if [ -n "$body" ]; then
    curl -s -i -X "$method" "$BASE_URL$path" \
      -H "X-Gateway-Token: $TOKEN" -H "X-Caller-UserId: u1" \
      -H "X-Caller-Email: admin@example.com" -H "X-Caller-Role: admin" \
      -H "Content-Type: application/json" --data "$body" --max-time 10
  else
    curl -s -i -X "$method" "$BASE_URL$path" \
      -H "X-Gateway-Token: $TOKEN" -H "X-Caller-UserId: u1" \
      -H "X-Caller-Email: admin@example.com" -H "X-Caller-Role: admin" --max-time 10
  fi
}

http_status() { sed -n '1{s/^HTTP\/[0-9.]* \([0-9]*\).*/\1/p}'; }

# d1_exec <sql> -- runs against the scratch DB, returns pure JSON on
# stdout. `--json` is required, not cosmetic: without it, wrangler
# writes its banner ("wrangler 4.x", "Resource location: local", ...)
# to stdout ahead of the JSON, which breaks every downstream `jq`
# parse -- confirmed by reproducing it directly. Same flag
# scripts/classify-audit-schema.sh already uses for the same reason.
d1_exec() {
  local out rc
  out=$(cd "$CORE_DIR" && wrangler d1 execute --local "$DB_NAME" --json --command "$1" 2>"$WORKDIR/d1.err")
  rc=$?
  if [ $rc -ne 0 ]; then
    echo "d1_exec: wrangler exited $rc for: $1" >&2
    cat "$WORKDIR/d1.err" >&2
    fail "d1_exec failed: $1"
  fi
  printf '%s' "$out"
}

# d1_count <table> <where> -- integer row count
d1_count() {
  d1_exec "SELECT COUNT(*) AS n FROM $1 WHERE $2" | jq -r '.[0].results[0].n'
}

seed_fixture() {
  d1_exec "INSERT INTO users (id, email, name, role) VALUES ('u1','admin@example.com','Admin','admin')" >/dev/null
}

seed_fixture

# ═══════════════════════════════════════════════════════════════════
# (a) G-22 — replace preserves check results, incidents, attachments
# ═══════════════════════════════════════════════════════════════════

TID_A="a-$$-target"
d1_exec "INSERT INTO targets (id, name, type, host, is_disabled, owner_id, created_at, updated_at, created_by, updated_by, success_threshold, failure_threshold) VALUES ('$TID_A','a-target','https','a.example.com',0,'u1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','u1','u1',3,3)" >/dev/null
d1_exec "INSERT INTO target_states (target_id, current_status) VALUES ('$TID_A','up')" >/dev/null
d1_exec "INSERT INTO notification_channels (id, name, channel_type, endpoint, owner_id, created_at) VALUES ('a-c1','c1','webhook','https://h/1','u1','2026-01-01T00:00:00Z'), ('a-c2','c2','webhook','https://h/2','u1','2026-01-01T00:00:00Z')" >/dev/null
d1_exec "INSERT INTO target_notifications (target_id, channel_id, on_down, on_up) VALUES ('$TID_A','a-c1',1,0), ('$TID_A','a-c2',1,1)" >/dev/null
d1_exec "INSERT INTO check_results (id, target_id, checked_at, is_success, status_code) VALUES ('a-r1','$TID_A','2026-01-01T00:00:00Z',1,200), ('a-r2','$TID_A','2026-01-01T01:00:00Z',1,200)" >/dev/null
d1_exec "INSERT INTO incidents (id, target_id, status, opened_at, cause) VALUES ('a-i1','$TID_A','open','2026-01-01T02:00:00Z','timeout')" >/dev/null

BEFORE_RESULTS=$(d1_count check_results "target_id='$TID_A'") || fail "(a): could not read baseline check_results count"
BEFORE_INCIDENTS=$(d1_count incidents "target_id='$TID_A'") || fail "(a): could not read baseline incidents count"
BEFORE_LINKS=$(d1_count target_notifications "target_id='$TID_A'") || fail "(a): could not read baseline target_notifications count"
BEFORE_STATES=$(d1_count target_states "target_id='$TID_A'") || fail "(a): could not read baseline target_states count"

IMPORT_A=$(cat <<JSON
{"payload":{"schema_version":1,"exported_at":"2026-01-01T00:00:00Z","source_deployment":"other",
 "data":{"targets":[{"id":"$TID_A","name":"a-target (re-imported)","type":"https","host":"a.example.com",
 "port":null,"path":"/","expected_status":200,"body_contains":null,"tls_threshold_days":30,
 "timeout_sec":10,"retry_count":3,"interval_minutes":5,"is_disabled":false,"owner_id":"u1","tags":null,
 "next_check_at":"2026-01-01T00:00:00Z","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z",
 "created_by":"ignored","updated_by":"ignored","success_threshold":3,"failure_threshold":3}],
 "channels":[],"target_notifications":[],"maintenance_windows":[],"users":null}},
 "on_conflict":"replace","apply":true}
JSON
)
RESP_A=$(admin_req POST /admin/migration/import "$IMPORT_A")
STATUS_A=$(echo "$RESP_A" | http_status)
[ "$STATUS_A" = "200" ] || { echo "$RESP_A" >&2; fail "(a): replace import returned $STATUS_A, expected 200"; }

AFTER_RESULTS=$(d1_count check_results "target_id='$TID_A'") || fail "(a): could not read post-import check_results count"
AFTER_INCIDENTS=$(d1_count incidents "target_id='$TID_A'") || fail "(a): could not read post-import incidents count"
AFTER_LINKS=$(d1_count target_notifications "target_id='$TID_A'") || fail "(a): could not read post-import target_notifications count"
AFTER_STATES=$(d1_count target_states "target_id='$TID_A'") || fail "(a): could not read post-import target_states count"

[ "$BEFORE_RESULTS" = "$AFTER_RESULTS" ] || fail "(a) G-22: check_results count changed on replace ($BEFORE_RESULTS -> $AFTER_RESULTS) -- cascade fired"
[ "$BEFORE_INCIDENTS" = "$AFTER_INCIDENTS" ] || fail "(a) G-22: incidents count changed on replace ($BEFORE_INCIDENTS -> $AFTER_INCIDENTS) -- cascade fired"
[ "$BEFORE_LINKS" = "$AFTER_LINKS" ] || fail "(a) G-22: target_notifications count changed on replace ($BEFORE_LINKS -> $AFTER_LINKS) -- cascade fired"
[ "$BEFORE_STATES" = "$AFTER_STATES" ] || fail "(a) G-22: target_states count changed on replace ($BEFORE_STATES -> $AFTER_STATES) -- cascade fired"
[ "$AFTER_RESULTS" = "2" ] || fail "(a) G-22: expected 2 check_results, found $AFTER_RESULTS"
echo "PASS (a) G-22: replace preserves check results ($AFTER_RESULTS), incidents ($AFTER_INCIDENTS), attachments ($AFTER_LINKS), state ($AFTER_STATES)"

# ═══════════════════════════════════════════════════════════════════
# (b) G-31 — an unresolvable owner is refused before any write
# ═══════════════════════════════════════════════════════════════════

TID_B="b-$$-ghost"
IMPORT_B=$(cat <<JSON
{"payload":{"schema_version":1,"exported_at":"2026-01-01T00:00:00Z","source_deployment":"other",
 "data":{"targets":[{"id":"$TID_B","name":"ghost","type":"https","host":"ghost.example.com",
 "port":null,"path":"/","expected_status":200,"body_contains":null,"tls_threshold_days":30,
 "timeout_sec":10,"retry_count":3,"interval_minutes":5,"is_disabled":false,"owner_id":"u-ghost","tags":null,
 "next_check_at":"2026-01-01T00:00:00Z","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z",
 "created_by":"x","updated_by":"x","success_threshold":3,"failure_threshold":3}],
 "channels":[],"target_notifications":[],"maintenance_windows":[],"users":null}},
 "on_conflict":"skip","apply":true}
JSON
)
RESP_B=$(admin_req POST /admin/migration/import "$IMPORT_B")
STATUS_B=$(echo "$RESP_B" | http_status)
[ "$STATUS_B" -ge 400 ] 2>/dev/null || { echo "$RESP_B" >&2; fail "(b) G-31: unresolvable-owner import returned $STATUS_B, expected an error"; }

WRITTEN_B=$(d1_count targets "id='$TID_B'") || fail "(b): could not read targets count for $TID_B"
[ "$WRITTEN_B" = "0" ] || fail "(b) G-31: target $TID_B was written despite an unresolvable owner"

# Dry run must fail identically, having written nothing (FR-MIG-05/06).
IMPORT_B_DRY=$(echo "$IMPORT_B" | sed 's/"apply":true/"apply":false/')
RESP_B_DRY=$(admin_req POST /admin/migration/import "$IMPORT_B_DRY")
STATUS_B_DRY=$(echo "$RESP_B_DRY" | http_status)
[ "$STATUS_B_DRY" -ge 400 ] 2>/dev/null || { echo "$RESP_B_DRY" >&2; fail "(b) G-31: dry-run unresolvable-owner import returned $STATUS_B_DRY, expected an error"; }
echo "PASS (b) G-31: an unresolvable owner is refused before any write, applied ($STATUS_B) and dry run ($STATUS_B_DRY) alike"

# ═══════════════════════════════════════════════════════════════════
# (c) G-06 — an imported target gets a target_states row and is
# monitorable: a real scheduled tick probes it and failure_threshold=1
# produces `down` after exactly one failed check
# ═══════════════════════════════════════════════════════════════════

TID_C="c-$$-monitor"
IMPORT_C=$(cat <<JSON
{"payload":{"schema_version":1,"exported_at":"2026-01-01T00:00:00Z","source_deployment":"other",
 "data":{"targets":[{"id":"$TID_C","name":"c-target","type":"tcp","host":"127.0.0.1",
 "port":1,"path":null,"expected_status":null,"body_contains":null,"tls_threshold_days":null,
 "timeout_sec":3,"retry_count":0,"interval_minutes":5,"is_disabled":false,"owner_id":"u1","tags":null,
 "next_check_at":"2020-01-01T00:00:00Z","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z",
 "created_by":"x","updated_by":"x","success_threshold":3,"failure_threshold":1}],
 "channels":[],"target_notifications":[],"maintenance_windows":[],"users":null}},
 "on_conflict":"skip","apply":true}
JSON
)
RESP_C=$(admin_req POST /admin/migration/import "$IMPORT_C")
STATUS_C=$(echo "$RESP_C" | http_status)
[ "$STATUS_C" = "200" ] || { echo "$RESP_C" >&2; fail "(c) G-06: import returned $STATUS_C, expected 200"; }

STATE_ROWS_C=$(d1_count target_states "target_id='$TID_C'") || fail "(c): could not read target_states count for $TID_C"
[ "$STATE_ROWS_C" = "1" ] || fail "(c) G-06: expected exactly one target_states row after import, found $STATE_ROWS_C"

STATUS_BEFORE_C=$(d1_exec "SELECT current_status FROM target_states WHERE target_id='$TID_C'" | jq -r '.[0].results[0].current_status') \
  || fail "(c): could not read initial current_status for $TID_C"
[ "$STATUS_BEFORE_C" = "unknown" ] || fail "(c) G-06: imported target's initial status was '$STATUS_BEFORE_C', expected 'unknown'"

curl -s "$BASE_URL/cdn-cgi/handler/scheduled" --max-time 15 >/dev/null 2>&1
sleep 1

PROBED_C=$(d1_count check_results "target_id='$TID_C'") || fail "(c): could not read check_results count for $TID_C"
[ "$PROBED_C" -ge 1 ] 2>/dev/null || fail "(c) G-06: imported target was not probed by the scheduled tick (0 check_results)"

STATUS_AFTER_C=$(d1_exec "SELECT current_status, consecutive_failures FROM target_states WHERE target_id='$TID_C'" | jq -r '.[0].results[0].current_status') \
  || fail "(c): could not read post-tick current_status for $TID_C"
FAILURES_AFTER_C=$(d1_exec "SELECT consecutive_failures FROM target_states WHERE target_id='$TID_C'" | jq -r '.[0].results[0].consecutive_failures') \
  || fail "(c): could not read post-tick consecutive_failures for $TID_C"
[ "$STATUS_AFTER_C" = "down" ] || fail "(c) G-06: expected 'down' after one failed check with failure_threshold=1, got '$STATUS_AFTER_C'"
[ "$FAILURES_AFTER_C" = "1" ] || fail "(c) G-06: expected consecutive_failures=1, got $FAILURES_AFTER_C"
echo "PASS (c) G-06: imported target got a target_states row, was selected and probed by a real scheduled tick, and transitioned to down after exactly one failed check (failure_threshold=1)"

echo
echo "All D1-behaviour gate checks passed."
