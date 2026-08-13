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
#       (`/__scheduled`) selects and probes it, and an imported
#       failure_threshold=1 produces `down` after exactly one failed
#       check
#   (d) T-227 (subject 07g) — a scheduled tick, driven at whatever
#       minute the real wall clock happens to be, makes exactly one of
#       two things observable: retention ran (an eligible row was
#       deleted), or the skip was logged, naming the minute. Never
#       neither -- that silent-neither outcome is G-43 itself.
#   (e) T-52b (subject 11, G-07) — a window with suppress_notify = 0
#       does not suppress a notification, and one with
#       exclude_from_sla = 0 does not move the SLA figure (plus a
#       differential guard: exclude_from_sla = 1 DOES move it)
#   (f) T-66b (subject 12, G-09/G-27) — a window scoped to tag `api`
#       does not suppress a target tagged `api-v2` (no substring
#       leakage), and one scoped to a tag containing `%` matches
#       nothing but a target tagged exactly `%` (no wildcard leakage,
#       exact matching still works for the literal case)
#   (g) T-70a (subject 13, G-12) — a maintenance window covering the
#       entire report period excludes the whole denominator and
#       reports SLA as not applicable (JSON null), not a claimed
#       100%, through the real HTTP + D1 path
#
# T-226, DR-LIF-06 (a retention pass deletes *only* what it archived,
# driven by a controlled nominal time), is still NOT included, even
# after subject 07g. `db::retention::run_cleanup`'s caller now decides
# from `event.schedule()`, not the wall clock (G-43, closed) -- but
# under `wrangler dev --local`, `event.schedule()` **is** the wall
# clock: confirmed against real `workerd` that the `--test-scheduled`
# harness's nominal-time override never reaches the compiled Worker
# (`.git-exclude/reviewed/058-subject-07g-escalation-ruling.md` traces
# it to workerd's local scheduled-event simulation, not this project's
# code or the `worker` crate). So this gate still cannot drive a
# retention pass *on demand* -- only observe whichever of the two
# outcomes the real clock happens to produce at run time, which is
# exactly what (d) does. Recorded honestly rather than papered over
# (the G-32/G-33 lesson): an assertion that appears to test DR-LIF-06
# but cannot actually trigger the pass would be worse than the gap.
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

# http_body -- strips the status line and headers off a `curl -i`
# response, leaving just the (single-line JSON) body. Needed from (e)
# onward, which reads IDs and report fields back out of responses
# rather than only checking the status code.
http_body() { awk 'body{print} /^\r?$/{body=1}'; }

# iso_offset <seconds> -- UTC ISO-8601 timestamp `seconds` from now
# (negative for the past). Used to build maintenance windows that
# bracket "now" without this script needing to know Core's clock any
# more precisely than the host clock it already runs on.
iso_offset() { date -u -d "@$(($(date +%s) + $1))" +%Y-%m-%dT%H:%M:%SZ; }

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

trigger_scheduled_tick() {
  # `/__scheduled` is the documented endpoint (`wrangler dev --help`).
  # `/cdn-cgi/handler/scheduled`, used earlier in this project's
  # history, answers 200 "ok" but never reaches the compiled Worker at
  # all when given query parameters, and reaches it only by coincidence
  # when called bare -- confirmed during subject 07g's escalation. Use
  # the real one everywhere.
  curl -s "$BASE_URL/__scheduled" --max-time 15
}

trigger_scheduled_tick >/dev/null 2>&1
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

# ═══════════════════════════════════════════════════════════════════
# (d) T-227 (subject 07g, G-43) -- a scheduled tick makes exactly one
# of two things observable: retention ran (an eligible row was
# deleted), or the skip was logged, naming the minute. See this
# script's header for why DR-LIF-06 itself (T-226) still cannot be
# driven on demand.
# ═══════════════════════════════════════════════════════════════════

TID_D="d-$$-retention"
d1_exec "INSERT INTO targets (id, name, type, host, is_disabled, owner_id, created_at, updated_at, created_by, updated_by, success_threshold, failure_threshold) VALUES ('$TID_D','d-target','https','d.example.com',1,'u1','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z','u1','u1',3,3)" >/dev/null
d1_exec "INSERT INTO target_states (target_id, current_status) VALUES ('$TID_D','unknown')" >/dev/null
# retention_days=0: the cutoff is "now", so any existing row is already
# eligible. archive_to_r2=1 because check_results requires_archival
# (db/retention.rs) -- a policy with archive_to_r2=0 for this table is
# skipped with a warning, not obeyed, which would make this assertion
# meaningless. sql/0001_initial.sql already seeds a check_results row
# (90 days) -- UPDATE it rather than INSERT, which would collide with
# the table_name primary key.
d1_exec "UPDATE retention_policies SET retention_days = 0, archive_to_r2 = 1 WHERE table_name = 'check_results'" >/dev/null
d1_exec "INSERT INTO check_results (id, target_id, checked_at, is_success, status_code) VALUES ('d-r1','$TID_D','2020-01-01T00:00:00Z',1,200)" >/dev/null

BEFORE_D=$(d1_count check_results "id='d-r1'") || fail "(d): could not read baseline check_results count for d-r1"
[ "$BEFORE_D" = "1" ] || fail "(d): fixture setup failed, expected the eligible row to exist before the tick"

trigger_scheduled_tick >/dev/null 2>&1
sleep 1

AFTER_D=$(d1_count check_results "id='d-r1'") || fail "(d): could not read post-tick check_results count for d-r1"

if [ "$AFTER_D" = "0" ]; then
  echo "PASS (d) T-227: retention ran -- the eligible row was archived and deleted"
elif grep -q "Retention skipped this invocation: nominal schedule was minute" "$DEV_LOG"; then
  SKIP_LINE=$(grep "Retention skipped this invocation: nominal schedule was minute" "$DEV_LOG" | tail -1)
  echo "PASS (d) T-227: retention did not run this tick, and said so -- $SKIP_LINE"
else
  fail "(d) T-227: retention neither ran (row still present) nor logged a skip -- this is G-43's silent-neither outcome, the one this subject exists to eliminate"
fi

# ═══════════════════════════════════════════════════════════════════
# (e) T-52b (subject 11, G-07) — a window with suppress_notify = 0
# does not suppress a notification, and one with exclude_from_sla = 0
# does not move the SLA figure. `last_notification_at` on
# target_states is the observable: db/states.rs::mark_notified is
# only called when a notification actually dispatches (monitor/
# engine.rs), so it stays NULL exactly when suppression fired.
# ═══════════════════════════════════════════════════════════════════

TARGET_E_BODY=$(cat <<JSON
{"name":"e-target","type":"tcp","host":"127.0.0.1","port":1,"failure_threshold":1,"retry_count":0,"timeout_sec":1}
JSON
)
TID_E=$(admin_req POST /targets "$TARGET_E_BODY" | http_body | jq -r '.id')
[ -n "$TID_E" ] && [ "$TID_E" != "null" ] || fail "(e): could not create the target for the suppress_notify=0 check"

NOTIFIED_BEFORE_E=$(d1_exec "SELECT last_notification_at FROM target_states WHERE target_id='$TID_E'" | jq -r '.[0].results[0].last_notification_at')
[ "$NOTIFIED_BEFORE_E" = "null" ] || fail "(e): fixture setup -- expected last_notification_at to start NULL"

WIN_E_BODY=$(cat <<JSON
{"name":"non-suppressing window","start_at":"$(iso_offset -3600)","end_at":"$(iso_offset 3600)","target_id":"$TID_E","suppress_notify":false,"exclude_from_sla":true}
JSON
)
RESP_WIN_E=$(admin_req POST /maintenance "$WIN_E_BODY")
STATUS_WIN_E=$(echo "$RESP_WIN_E" | http_status)
[ "$STATUS_WIN_E" = "200" ] || { echo "$RESP_WIN_E" >&2; fail "(e): creating the suppress_notify=false window returned $STATUS_WIN_E"; }

trigger_scheduled_tick >/dev/null 2>&1
sleep 1

NOTIFIED_AFTER_E=$(d1_exec "SELECT last_notification_at FROM target_states WHERE target_id='$TID_E'" | jq -r '.[0].results[0].last_notification_at')
[ "$NOTIFIED_AFTER_E" != "null" ] || fail "(e) T-52: a suppress_notify=0 window silenced the notification -- it must not"
echo "PASS (e) T-52: a window with suppress_notify=0 does not suppress a notification (last_notification_at=$NOTIFIED_AFTER_E)"

TARGET_E2_BODY=$(cat <<JSON
{"name":"e2-target","type":"https","host":"e2.example.com","success_threshold":3,"failure_threshold":3}
JSON
)
TID_E2=$(admin_req POST /targets "$TARGET_E2_BODY" | http_body | jq -r '.id')
[ -n "$TID_E2" ] && [ "$TID_E2" != "null" ] || fail "(e): could not create the target for the exclude_from_sla=0 check"

WIN_E2_BODY=$(cat <<JSON
{"name":"non-excluding window","start_at":"$(iso_offset -3600)","end_at":"$(iso_offset 3600)","target_id":"$TID_E2","suppress_notify":true,"exclude_from_sla":false}
JSON
)
RESP_WIN_E2=$(admin_req POST /maintenance "$WIN_E2_BODY")
STATUS_WIN_E2=$(echo "$RESP_WIN_E2" | http_status)
[ "$STATUS_WIN_E2" = "200" ] || { echo "$RESP_WIN_E2" >&2; fail "(e): creating the exclude_from_sla=false window returned $STATUS_WIN_E2"; }

SLA_E2=$(admin_req GET "/targets/$TID_E2/sla")
[ "$(echo "$SLA_E2" | http_status)" = "200" ] || { echo "$SLA_E2" >&2; fail "(e): SLA fetch for $TID_E2 failed"; }
EXCLUDED_E2=$(echo "$SLA_E2" | http_body | jq -r '.excluded_seconds')
[ "$EXCLUDED_E2" = "0" ] || fail "(e) T-52: an exclude_from_sla=0 window changed the SLA figure -- excluded_seconds=$EXCLUDED_E2, expected 0"
echo "PASS (e) T-52: a window with exclude_from_sla=0 does not move the SLA figure (excluded_seconds=$EXCLUDED_E2)"

# Differential guard: the same shape of window with exclude_from_sla=1
# DOES move the figure -- otherwise (e) would pass vacuously against a
# filter that always excludes list_in_window's results.
TARGET_E3_BODY=$(cat <<JSON
{"name":"e3-target","type":"https","host":"e3.example.com","success_threshold":3,"failure_threshold":3}
JSON
)
TID_E3=$(admin_req POST /targets "$TARGET_E3_BODY" | http_body | jq -r '.id')
[ -n "$TID_E3" ] && [ "$TID_E3" != "null" ] || fail "(e) guard: could not create the target for the exclude_from_sla=1 differential check"

WIN_E3_BODY=$(cat <<JSON
{"name":"excluding window","start_at":"$(iso_offset -3600)","end_at":"$(iso_offset 3600)","target_id":"$TID_E3","suppress_notify":true,"exclude_from_sla":true}
JSON
)
RESP_WIN_E3=$(admin_req POST /maintenance "$WIN_E3_BODY")
[ "$(echo "$RESP_WIN_E3" | http_status)" = "200" ] || { echo "$RESP_WIN_E3" >&2; fail "(e) guard: creating the exclude_from_sla=true window failed"; }

SLA_E3=$(admin_req GET "/targets/$TID_E3/sla")
EXCLUDED_E3=$(echo "$SLA_E3" | http_body | jq -r '.excluded_seconds')
[ "$EXCLUDED_E3" -gt 0 ] 2>/dev/null || fail "(e) guard: an exclude_from_sla=1 window did not move the SLA figure at all -- excluded_seconds=$EXCLUDED_E3, the filter may be inverted"
echo "PASS (e) guard: a window with exclude_from_sla=1 DOES move the SLA figure (excluded_seconds=$EXCLUDED_E3), confirming the flag actually differentiates"

# ═══════════════════════════════════════════════════════════════════
# (f) T-66b (subject 12, G-09/G-27) — a window scoped to tag `api`
# does not suppress a target tagged `api-v2` (no substring leakage),
# and one scoped to a tag containing `%` matches nothing but a target
# tagged exactly `%` (no wildcard leakage; exact matching still works
# for the literal-metacharacter case)
# ═══════════════════════════════════════════════════════════════════

TARGET_F1_BODY=$(cat <<JSON
{"name":"f1-target","type":"tcp","host":"127.0.0.1","port":1,"failure_threshold":1,"retry_count":0,"timeout_sec":1,"tags":"[\"api-v2\"]"}
JSON
)
TID_F1=$(admin_req POST /targets "$TARGET_F1_BODY" | http_body | jq -r '.id')
[ -n "$TID_F1" ] && [ "$TID_F1" != "null" ] || fail "(f): could not create the api-v2-tagged target"

TARGET_F2_BODY=$(cat <<JSON
{"name":"f2-target","type":"tcp","host":"127.0.0.1","port":1,"failure_threshold":1,"retry_count":0,"timeout_sec":1,"tags":"[\"prod\"]"}
JSON
)
TID_F2=$(admin_req POST /targets "$TARGET_F2_BODY" | http_body | jq -r '.id')
[ -n "$TID_F2" ] && [ "$TID_F2" != "null" ] || fail "(f): could not create the prod-tagged target"

TARGET_F3_BODY=$(cat <<JSON
{"name":"f3-target","type":"tcp","host":"127.0.0.1","port":1,"failure_threshold":1,"retry_count":0,"timeout_sec":1,"tags":"[\"%\"]"}
JSON
)
TID_F3=$(admin_req POST /targets "$TARGET_F3_BODY" | http_body | jq -r '.id')
[ -n "$TID_F3" ] && [ "$TID_F3" != "null" ] || fail "(f): could not create the literal-percent-tagged target"

WIN_F_API_BODY=$(cat <<JSON
{"name":"scoped to api","start_at":"$(iso_offset -3600)","end_at":"$(iso_offset 3600)","target_tag":"api","suppress_notify":true,"exclude_from_sla":true}
JSON
)
RESP_WIN_F_API=$(admin_req POST /maintenance "$WIN_F_API_BODY")
[ "$(echo "$RESP_WIN_F_API" | http_status)" = "200" ] || { echo "$RESP_WIN_F_API" >&2; fail "(f): creating the tag=api window failed"; }

WIN_F_PCT_BODY=$(cat <<JSON
{"name":"scoped to percent","start_at":"$(iso_offset -3600)","end_at":"$(iso_offset 3600)","target_tag":"%","suppress_notify":true,"exclude_from_sla":true}
JSON
)
RESP_WIN_F_PCT=$(admin_req POST /maintenance "$WIN_F_PCT_BODY")
[ "$(echo "$RESP_WIN_F_PCT" | http_status)" = "200" ] || { echo "$RESP_WIN_F_PCT" >&2; fail "(f): creating the tag=% window failed"; }

trigger_scheduled_tick >/dev/null 2>&1
sleep 1

NOTIFIED_F1=$(d1_exec "SELECT last_notification_at FROM target_states WHERE target_id='$TID_F1'" | jq -r '.[0].results[0].last_notification_at')
[ "$NOTIFIED_F1" != "null" ] || fail "(f) T-58/G-09: a window scoped to tag 'api' suppressed a target tagged 'api-v2' -- substring leakage"

NOTIFIED_F2=$(d1_exec "SELECT last_notification_at FROM target_states WHERE target_id='$TID_F2'" | jq -r '.[0].results[0].last_notification_at')
[ "$NOTIFIED_F2" != "null" ] || fail "(f) T-60/G-27: a window scoped to tag '%' suppressed a target tagged 'prod' -- wildcard leakage"

NOTIFIED_F3=$(d1_exec "SELECT last_notification_at FROM target_states WHERE target_id='$TID_F3'" | jq -r '.[0].results[0].last_notification_at')
[ "$NOTIFIED_F3" = "null" ] || fail "(f) guard: a window scoped to tag '%' failed to suppress a target tagged exactly '%' -- exact matching may be broken entirely"

echo "PASS (f) T-66b: tag 'api' does not match 'api-v2' (G-09), tag '%' does not match 'prod' (G-27), and tag '%' still matches a target tagged exactly '%' (exact matching intact)"

# ═══════════════════════════════════════════════════════════════════
# (g) T-70a (subject 13, G-12) — a window covering the entire report
# period excludes the whole denominator and reports SLA as not
# applicable (JSON null), not a claimed 100%, through the real
# HTTP + D1 path (the arithmetic itself is T-67..T-71 in
# crates/core/src/stats.rs -- this proves the wiring, not the math)
# ═══════════════════════════════════════════════════════════════════

TARGET_G_BODY=$(cat <<JSON
{"name":"g-target","type":"https","host":"g.example.com","success_threshold":3,"failure_threshold":3}
JSON
)
TID_G=$(admin_req POST /targets "$TARGET_G_BODY" | http_body | jq -r '.id')
[ -n "$TID_G" ] && [ "$TID_G" != "null" ] || fail "(g): could not create the target for the fully-excluded-window check"

WIN_G_BODY=$(cat <<JSON
{"name":"fully excluding window","start_at":"$(iso_offset -172800)","end_at":"$(iso_offset 172800)","target_id":"$TID_G","suppress_notify":true,"exclude_from_sla":true}
JSON
)
RESP_WIN_G=$(admin_req POST /maintenance "$WIN_G_BODY")
[ "$(echo "$RESP_WIN_G" | http_status)" = "200" ] || { echo "$RESP_WIN_G" >&2; fail "(g): creating the fully-excluding window failed"; }

SLA_G=$(admin_req GET "/targets/$TID_G/sla")
[ "$(echo "$SLA_G" | http_status)" = "200" ] || { echo "$SLA_G" >&2; fail "(g): SLA fetch for $TID_G failed"; }
SLA_BODY_G=$(echo "$SLA_G" | http_body)
WINDOW_SECONDS_G=$(echo "$SLA_BODY_G" | jq -r '.window_seconds')
EXCLUDED_G=$(echo "$SLA_BODY_G" | jq -r '.excluded_seconds')
RATIO_G=$(echo "$SLA_BODY_G" | jq -c '.sla_uptime_ratio')

[ "$EXCLUDED_G" = "$WINDOW_SECONDS_G" ] || fail "(g) T-70a: excluded_seconds ($EXCLUDED_G) did not account for the whole window ($WINDOW_SECONDS_G)"
[ "$RATIO_G" = "null" ] || fail "(g) T-70a: a fully-excluded window reported sla_uptime_ratio=$RATIO_G, expected null (not applicable), not a claimed percentage"
echo "PASS (g) T-70a: a window covering the entire report period excludes the whole denominator (excluded_seconds=$EXCLUDED_G of $WINDOW_SECONDS_G) and reports SLA as not applicable (null)"

echo
echo "All D1-behaviour gate checks passed."
