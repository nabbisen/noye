#!/usr/bin/env bash
# Subject 07a, Step 3, item #5 (DEC-020) --
# rfcs/handoffs/07a-live-residual-triage.md.
#
# Measures current_head_hash's per-write cost -- it loads and walks
# the ENTIRE audit_logs table on every single audit write
# (crates/core/src/db/audit.rs: "SELECT * FROM audit_logs ORDER BY
# action_time ASC", no LIMIT) -- against real D1 at realistic table
# sizes. DEC-020 requires this be measured against live D1: local
# emulation's timing has no relationship to a real network round trip
# ("a timing number from in-process SQLite has no relationship to a
# network round-trip to D1" -- ruling 036 §1, made about a different
# number but the same principle).
#
# This script only handles the part that's safe to script: bulk
# seeding synthetic rows to reach a target table size, and cleaning
# them up afterward. Seeded rows do NOT need to form a valid hash
# chain to be a realistic measurement -- current_head_hash's query has
# no WHERE clause, so it fetches and deserializes every row
# regardless of whether that row is reachable from genesis. The
# dominant cost this is measuring (D1 round-trip + row transfer +
# deserialization) scales with total row count, connected or not.
# Disconnected seeded rows show up as "orphaned" if you run
# `GET /api/admin/audit/verify` afterward -- expected, harmless, and
# gone as soon as `cleanup` runs.
#
# The actual TIMING step needs a real authenticated write against
# your real Gateway, which this script cannot do for you (Core is not
# publicly reachable at all -- only via the Gateway's authenticated
# session or the Service Binding -- and scripting your OIDC login is
# out of scope here). Use your browser instead: log in as usual, open
# DevTools' Network tab, perform one write (e.g. create a throwaway
# target, or anything that calls db::audit::log), and read that
# request's time directly. That is simpler and more reliable than
# scripting session-cookie extraction.
#
# Usage:
#   02-audit-chain-write-cost.sh --local|--remote seed <user-id> <target-count>
#   02-audit-chain-write-cost.sh --local|--remote count
#   02-audit-chain-write-cost.sh --local|--remote cleanup
#
# --local proves this script's SQL is correct (T-187) against
#   `wrangler dev --local`; the timing number you'd get locally is not
#   the deliverable and should not be recorded as DEC-020's answer.
#
# <user-id> must be a real, already-existing row in your `users`
# table (audit_logs.actor_id is a foreign key) -- pick your own admin
# user id; this script never creates one.
#
# <target-count> is the total audit_logs row count you want to reach,
# not an additional amount to add -- the script tops up from whatever
# is there now. Suggested checkpoints: run this at 1,000, then again
# at 10,000, then again at 50,000, timing one real write at each
# stop, so the capture form shows whether cost scales the way an
# O(n) full-table walk predicts.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CORE_DIR="$REPO_ROOT/crates/core"
ID_PREFIX="verify-audit-cost-"

fail() {
    echo "ERROR: $*" >&2
    exit 1
}

MODE=""
case "${1:-}" in
    --local) MODE="--local"; shift ;;
    --remote) MODE="--remote"; shift ;;
    *) fail "first argument must be --local or --remote" ;;
esac

DB_ARGS=(d1 execute noye_db "$MODE")
if [ "$MODE" = "--local" ]; then
    DB_ARGS+=(--persist-to "$REPO_ROOT/.git-exclude/tmp/deployment-verify-02-state")
fi

run_sql() {
    (cd "$CORE_DIR" && wrangler "${DB_ARGS[@]}" --command "$1")
}

total_count() {
    run_sql "SELECT COUNT(*) AS n FROM audit_logs"
}

SUBCOMMAND="${1:-}"
case "$SUBCOMMAND" in
    seed)
        USER_ID="${2:-}"
        TARGET_COUNT="${3:-}"
        [ -n "$USER_ID" ] && [ -n "$TARGET_COUNT" ] || \
            fail "seed requires a user id and a target total count: $0 $MODE seed <user-id> <target-count>"

        echo "audit_logs count before seeding:"
        total_count

        # "Top up" means starting from however many verify- rows
        # already exist, not always inserting ids 1..TARGET_COUNT --
        # calling seed twice with the same TARGET_COUNT must be a
        # no-op the second time, not a primary-key collision.
        ALREADY="$(run_sql "SELECT COUNT(*) AS n FROM audit_logs WHERE id LIKE '${ID_PREFIX}%'" \
            | grep -o '"n": *[0-9]*' | grep -o '[0-9]*')"
        ALREADY="${ALREADY:-0}"
        echo "Already-seeded verify rows: $ALREADY"
        if [ "$ALREADY" -ge "$TARGET_COUNT" ]; then
            echo "Already at or past the target count -- nothing to do."
        fi

        # Chunk size: DIFFERENT limit from DEC-017's bound-parameter
        # ceiling (no bind parameters here at all -- literals are
        # inlined). Empirically, local D1 rejects a single multi-row
        # VALUES INSERT somewhere between 300 and 500 rows with
        # "statement too long: SQLITE_TOOBIG" (SQLite's default
        # SQLITE_LIMIT_COMPOUND_SELECT=500 treats a large VALUES list
        # as a compound SELECT). 100 reuses this project's own
        # already-measured-safe RETENTION_BATCH_SIZE.
        CHUNK=100
        i="$ALREADY"
        while [ "$i" -lt "$TARGET_COUNT" ]; do
            this_chunk=$(( TARGET_COUNT - i < CHUNK ? TARGET_COUNT - i : CHUNK ))
            values=""
            for j in $(seq 1 "$this_chunk"); do
                sep=","
                [ "$j" -eq 1 ] && sep=""
                # Distinct, arbitrary, deliberately NOT chained to the
                # real head or to each other -- see the header comment
                # for why that's fine for this measurement.
                n=$((i + j))
                rh="verifyrowhash${n}$(printf '%040d' "$n")"
                ph="verifyprevhash${n}$(printf '%040d' "$n")"
                values="${values}${sep}('${ID_PREFIX}${n}',datetime('now'),'${USER_ID}','verify@example.invalid','verify','verify-${n}','seed','success','${ph}','${rh}')"
            done
            run_sql "INSERT INTO audit_logs (id, action_time, actor_id, actor_email, resource_type, resource_id, action_type, result, prev_hash, row_hash) VALUES ${values}"
            i=$((i + this_chunk))
            echo "  inserted so far this run: $i"
        done

        echo
        echo "audit_logs count after seeding:"
        total_count
        echo
        echo "Now, in your browser (logged in as usual): open DevTools' Network"
        echo "tab, perform one write (e.g. create a throwaway target), and read"
        echo "that request's total time."
        echo
        echo "CAPTURE FORM -- paste this into the evidence log verbatim:"
        echo "  audit_logs row count at this checkpoint: <from the count above>"
        echo "  action performed:                        <e.g. POST /targets>"
        echo "  observed request time (ms):               <from DevTools Network tab>"
        ;;

    count)
        total_count
        ;;

    cleanup)
        echo "Deleting all verify-audit-cost- rows..."
        run_sql "DELETE FROM audit_logs WHERE id LIKE '${ID_PREFIX}%'"
        echo "Done. Real audit rows (yours) are untouched -- only the id prefix"
        echo "this script uses was ever selected."
        echo
        echo "If you ran GET /api/admin/audit/verify while seeded rows were"
        echo "present, it will have reported them as orphaned -- re-run it now"
        echo "to confirm your real chain's report is unaffected."
        ;;

    *)
        fail "usage: $0 --local|--remote seed <user-id> <target-count> | count | cleanup"
        ;;
esac
