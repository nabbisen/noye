#!/usr/bin/env bash
# Subject 07a, Step 3, item #1 (DEC-017's remaining half) --
# rfcs/handoffs/07a-live-residual-triage.md.
#
# Measures how many retention batches (RETENTION_BATCH_SIZE=100 rows
# each) complete in a SINGLE scheduled invocation before Cloudflare's
# real Workers subrequest/CPU-time budget cuts it off. This cannot be
# answered against `wrangler dev --local` -- the local runtime enforces
# none of the platform limits this number depends on (DEC-017's own
# "LOCAL-PARTIAL" verdict, confirmed by subject 07a's Step 1 triage:
# the bound-parameter ceiling is a SQLite compile-time property and
# was measured locally; this is a Workers platform quota and is not).
#
# Retention only runs at minute 0 of every hour
# (crates/core/src/monitor/engine.rs, "runs at minute 0 of every
# hour"), so this is a two-step, wall-clock-spanning measurement:
# seed a large backlog, wait for the next :00 tick to fire and finish,
# then read how much of the backlog it cleared.
#
# Usage:
#   01-retention-batches-per-invocation.sh --local|--remote <db-name> seed <target-id> [count]
#   01-retention-batches-per-invocation.sh --local|--remote <db-name> check
#   01-retention-batches-per-invocation.sh --local|--remote <db-name> cleanup
#
# --local runs against `wrangler dev --local` (proves this script's
#   SQL and control flow are correct -- T-187 -- but the *number* it
#   produces locally is meaningless: there is no subrequest budget to
#   hit). --remote is the real measurement, against your actual
#   deployed database, and is the only mode that answers DEC-017.
#
# <db-name> has NO default and is never assumed to be "noye_db" --
# this script writes and deletes thousands of rows, and pointing it at
# a real deployment's database by accident is not a recoverable
# mistake. **Strongly recommended**: run this against a scratch
# deployment you provision as part of `03-onboarding-checklist.md`,
# not against a database anything else depends on. If you deliberately
# choose to run it against a real deployment's database anyway, that
# is your call to make explicitly, every time, by typing the name.
#
# <target-id> must be a real, already-existing row in your `targets`
# table (check_results.target_id is a foreign key) -- this script
# does not create one, so it never adds a target you didn't already
# have. Find one with:
#   wrangler d1 execute <db-name> --remote --command "SELECT id FROM targets LIMIT 1"
#
# Seeded rows are tagged with the id prefix "verify-batch-" and dated
# safely before check_results' retention cutoff, so `check` (and, if
# you abort partway through, `cleanup`) can find exactly and only the
# rows this script added -- never your real backlog, if you have one.
#
# Cost note: every batch retention actually processes writes one real
# R2 object and deletes up to 100 real D1 rows. The default count
# below is deliberately large, to make it likely the run finds the
# real ceiling rather than exhausting the seeded backlog first (which
# would only tell you "at least N/100 batches complete," not the
# ceiling). Lower it if you are cost-conscious; a smaller number is a
# valid, just less informative, result.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CORE_DIR="$REPO_ROOT/crates/core"
ID_PREFIX="verify-batch-"
DEFAULT_COUNT=20000

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

DB_NAME="${1:-}"
[ -n "$DB_NAME" ] || fail "second argument must be the D1 database name -- no default, never assumed to be your production database. See the header comment."
shift

DB_ARGS=(d1 execute "$DB_NAME" "$MODE")
if [ "$MODE" = "--local" ]; then
    # Matches this project's other local-verification scripts: an
    # isolated persist-to directory, never the ambient dev state.
    DB_ARGS+=(--persist-to "$REPO_ROOT/.git-exclude/tmp/deployment-verify-01-state")
fi

run_sql() {
    (cd "$CORE_DIR" && wrangler "${DB_ARGS[@]}" --command "$1")
}

count_eligible() {
    run_sql "SELECT COUNT(*) AS n FROM check_results WHERE id LIKE '${ID_PREFIX}%'"
}

SUBCOMMAND="${1:-}"
case "$SUBCOMMAND" in
    seed)
        TARGET_ID="${2:-}"
        COUNT="${3:-$DEFAULT_COUNT}"
        [ -n "$TARGET_ID" ] || fail "seed requires a target id: $0 $MODE $DB_NAME seed <target-id> [count]"

        echo "Eligible verify rows before seeding:"
        count_eligible

        # Build the INSERT in chunks. This is a DIFFERENT limit from
        # DEC-017's bound-parameter ceiling (these statements have no
        # bind parameters at all -- values are inlined literals):
        # empirically, local D1 rejects a single multi-row VALUES
        # INSERT somewhere between 300 and 500 rows with "statement
        # too long: SQLITE_TOOBIG" (SQLite's default
        # SQLITE_LIMIT_COMPOUND_SELECT=500 treats a large VALUES list
        # as a compound SELECT). 100 reuses this project's own
        # already-measured-safe RETENTION_BATCH_SIZE rather than
        # introducing a second unverified magic number.
        CHUNK=100
        i=0
        while [ "$i" -lt "$COUNT" ]; do
            this_chunk=$(( COUNT - i < CHUNK ? COUNT - i : CHUNK ))
            values=""
            for j in $(seq 1 "$this_chunk"); do
                n=$((i + j))
                sep=","
                [ "$j" -eq 1 ] && sep=""
                values="${values}${sep}('${ID_PREFIX}${n}','${TARGET_ID}',datetime('now','-100 days'),1,200)"
            done
            run_sql "INSERT INTO check_results (id, target_id, checked_at, is_success, status_code) VALUES ${values}"
            i=$((i + this_chunk))
            echo "  seeded $i / $COUNT"
        done

        echo
        echo "Eligible verify rows after seeding:"
        count_eligible
        echo
        echo "Now wait for the next :00-minute mark (top of the hour) to pass,"
        echo "plus a couple of minutes' margin, then run:"
        echo "  $0 $MODE $DB_NAME check"
        ;;

    check)
        echo "Eligible verify rows remaining right now:"
        count_eligible
        echo
        echo "CAPTURE FORM -- paste this into the evidence log verbatim:"
        echo "  seeded count:            <from the 'seed' step's output>"
        echo "  remaining count (above): <the number just printed>"
        echo "  rows processed:          seeded - remaining"
        echo "  batches per invocation:  rows processed / 100 (RETENTION_BATCH_SIZE)"
        echo "  wall-clock time of the :00 tick you waited for: <fill in>"
        ;;

    cleanup)
        echo "Deleting any remaining verify-batch- rows..."
        run_sql "DELETE FROM check_results WHERE id LIKE '${ID_PREFIX}%'"
        echo "Done. This does not touch anything retention already archived/deleted --"
        echo "those rows are gone from D1 either way; their R2 archive objects"
        echo "(archive/check_results/*.json) are untouched by this script."
        ;;

    *)
        fail "usage: $0 --local|--remote <db-name> seed <target-id> [count] | check | cleanup"
        ;;
esac
