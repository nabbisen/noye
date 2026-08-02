#!/usr/bin/env bash
# Audit-write-surfacing gate (subject: rfcs/handoffs/07-audit-write-surfacing.md).
#
# Static/structural checks only -- no D1 or Wrangler needed, and no browser
# needed (T-32's assertions live in crates/gateway's own `cargo test`, next
# to the scripts they check, since they're pure String assertions).
#
#   T-31 -- every db::audit::log_or_report call site in crates/core/src/api
#           that has an HTTP response to attach a warning to routes it
#           through api::with_audit_outcome. One documented exception:
#           channels.rs's send_test error branch already returns Err(e)
#           for an unrelated reason and has no successful response to
#           attach a warning header to.
#   T-35 -- no `let _ =` on any db::audit::log*/log_system* call remains
#           in the tree. Grep for the discard pattern, not for one
#           function name, so it catches both `log(...)` and
#           `log_system(...)`.
#
# T-31 and T-35 are must-fail-first: this script was written and run
# against the pre-fix tree (every one of the 17 sites still `let _ =`,
# no with_audit_outcome anywhere) and failed both checks. See
# .git-exclude/evidence/baseline-07.log.

set -u
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
API_DIR="$REPO_ROOT/crates/core/src/api"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

# ── T-35 — no `let _ =` on any db::audit::log*/log_system* call remains ──
DISCARDS="$(grep -rn 'let _ = db::audit::log' "$REPO_ROOT/crates" --include='*.rs' || true)"
if [ -n "$DISCARDS" ]; then
  fail "T-35: found a discarded db::audit::log*/log_system* call:
$DISCARDS"
fi
echo "PASS T-35: no \`let _ = db::audit::log*\` call remains in the tree"

# ── T-31 — every log_or_report call site with a response attaches the
#    audit-warning outcome ──
LOG_OR_REPORT_SITES="$(grep -rh 'db::audit::log_or_report(' "$API_DIR" --include='*.rs' | wc -l | tr -d ' ')"
WITH_OUTCOME_SITES="$(grep -rh 'api::with_audit_outcome(' "$API_DIR" --include='*.rs' | wc -l | tr -d ' ')"
# channels.rs's send_test error branch: already returns Err(e) for an
# unrelated failure, so there is no successful response to attach a
# warning to. Documented in .git-exclude/review-request/ for subject 07.
DOCUMENTED_EXCEPTIONS=1
EXPECTED=$((LOG_OR_REPORT_SITES - DOCUMENTED_EXCEPTIONS))
[ "$WITH_OUTCOME_SITES" -eq "$EXPECTED" ] || fail "T-31: expected $EXPECTED api::with_audit_outcome call site(s) ($LOG_OR_REPORT_SITES log_or_report site(s) minus $DOCUMENTED_EXCEPTIONS documented exception), found $WITH_OUTCOME_SITES"
[ "$LOG_OR_REPORT_SITES" -eq 15 ] || fail "T-31: expected exactly 15 log_or_report call sites (the handoff's count after pre-flight correction), found $LOG_OR_REPORT_SITES -- update this script's expectation deliberately if a call site was legitimately added or removed"
echo "PASS T-31: all $LOG_OR_REPORT_SITES log_or_report call sites accounted for; $WITH_OUTCOME_SITES attach the audit-warning outcome, $DOCUMENTED_EXCEPTIONS documented exception"

echo
echo "All audit-surfacing gate checks passed."
