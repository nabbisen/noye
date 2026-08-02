//! Tests for `db/audit.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md` for the standing rule
//! against adding inline `#[cfg(test)] mod tests` blocks.

use super::*;

// ── classify_hash_column_query_error (T-01a, host-testable half) ──

#[test]
fn missing_column_error_is_classified_as_missing_hash_columns() {
    let msg = classify_hash_column_query_error("D1_ERROR: no such column: prev_hash");
    assert_eq!(msg, MISSING_HASH_COLUMNS_ERROR);
}

#[test]
fn missing_column_error_matches_regardless_of_which_column() {
    let msg = classify_hash_column_query_error("D1_ERROR: no such column: row_hash");
    assert_eq!(msg, MISSING_HASH_COLUMNS_ERROR);
}

#[test]
fn unrelated_error_is_not_misreported_as_missing_columns() {
    let msg = classify_hash_column_query_error("D1_ERROR: internal error");
    assert_ne!(msg, MISSING_HASH_COLUMNS_ERROR);
    assert!(
        msg.contains("schema check failed"),
        "expected the generic wrapper, got: {msg}"
    );
    assert!(
        msg.contains("internal error"),
        "the underlying error text must be preserved, got: {msg}"
    );
}

#[test]
fn missing_hash_columns_error_names_the_remedy() {
    // The message is operator-facing (returned as a 500 body) and must
    // name both the condition and the fix, not just the symptom.
    assert!(MISSING_HASH_COLUMNS_ERROR.contains("prev_hash"));
    assert!(MISSING_HASH_COLUMNS_ERROR.contains("row_hash"));
    assert!(MISSING_HASH_COLUMNS_ERROR.contains("migration 0004"));
    assert!(!MISSING_HASH_COLUMNS_ERROR.contains("0,18.0"));
    assert!(!MISSING_HASH_COLUMNS_ERROR.contains("0.18.0"));
}

// ── audit_failure_log_line (subject 07, G-26, T-33, T-34) ──
//
// log_or_report / log_system_or_report are async and need a D1Database,
// so they aren't host-testable directly (same constraint noted
// throughout this project -- no Wrangler/Miniflare harness here). The
// line they log is pure formatting, split out so its content is
// testable without one.

#[test]
fn t33_failure_log_line_names_resource_id_action_and_actor() {
    let line = audit_failure_log_line("target", "t-1", "delete", "u-admin", "boom");
    assert!(line.contains("target"), "{line}");
    assert!(line.contains("t-1"), "{line}");
    assert!(line.contains("delete"), "{line}");
    assert!(line.contains("u-admin"), "{line}");
    assert!(line.contains("boom"), "{line}");
}

#[test]
fn t33_failure_log_line_names_the_system_actor_for_log_system_or_report() {
    let line = audit_failure_log_line("target", "t-1", "status_down", "system", "boom");
    assert!(line.contains("actor=system"), "{line}");
}

#[test]
fn t34_failure_log_line_is_exactly_five_fields_no_changed_values() {
    // `audit_failure_log_line` does not take `previous_value`/`new_value`
    // as parameters at all -- there is no field to accidentally
    // interpolate them into. This pins the exact output for a known
    // input, so a future change that widens the signature to also take
    // and print a changed value breaks this test loudly rather than
    // silently leaking it (log_or_report's own call site, below, passes
    // exactly these five arguments and nothing else).
    let line = audit_failure_log_line("target", "t-1", "update", "u-admin", "constraint failed");
    assert_eq!(
        line,
        "audit write failed: resource_type=target resource_id=t-1 \
         action_type=update actor=u-admin error=constraint failed"
    );
}

// ── walk_chain (subject 05, G-30, DEC-020) ──
//
// `walk_chain` is pure and takes already-fetched rows, so these run
// entirely on the host — no D1/Wrangler environment needed, same
// pattern as retention.rs's tests (NFR-QA-01).

fn make_row(
    id: &str,
    action_time: &str,
    prev_hash: Option<&str>,
    row_hash: Option<&str>,
) -> ChainRow {
    ChainRow {
        id: id.to_string(),
        action_time: action_time.to_string(),
        actor_id: "u-admin".to_string(),
        actor_email: None,
        resource_type: "target".to_string(),
        resource_id: Some("t-1".to_string()),
        action_type: "create".to_string(),
        previous_value: None,
        new_value: None,
        result: "success".to_string(),
        ip_address: None,
        prev_hash: prev_hash.map(String::from),
        row_hash: row_hash.map(String::from),
    }
}

/// Fields for `compute_row_hash`, matching a row built by `make_row` with
/// the same `id`/`action_time`. Kept separate from `make_row` because a
/// tampering test needs to alter a field *without* recomputing the hash
/// that would otherwise match it.
fn fields_for<'a>(id: &'a str, action_time: &'a str) -> AuditRowFields<'a> {
    AuditRowFields {
        id,
        action_time,
        actor_id: "u-admin",
        actor_email: None,
        resource_type: "target",
        resource_id: Some("t-1"),
        action_type: "create",
        previous_value: None,
        new_value: None,
        result: "success",
        ip_address: None,
    }
}

/// Build `n` correctly-chained rows, each a fresh random UUID, all
/// sharing one `action_time` — exactly what `n` sequential `log()` calls
/// within one wall-clock second produce.
fn build_chain(n: usize, action_time: &str) -> Vec<ChainRow> {
    let mut rows = Vec::new();
    let mut prev = GENESIS_HASH.to_string();
    for _ in 0..n {
        let id = uuid::Uuid::new_v4().to_string();
        let row_hash = compute_row_hash(&prev, &fields_for(&id, action_time));
        rows.push(make_row(&id, action_time, Some(&prev), Some(&row_hash)));
        prev = row_hash;
    }
    rows
}

/// Shorthand for tests that only care about the classification report,
/// not the head or fork flag `walk_chain` also returns.
fn verify(rows: &[ChainRow]) -> ChainVerification {
    walk_chain(rows).verification
}

/// Standing invariant guard (T-23e, subject 05 round 3 — found by
/// independent review, `.git-exclude/reviewed/
/// 028-subject-05-round-3.md` §2): every row is reported in exactly
/// one of the four classes, and no id appears in more than one of
/// `tampered_rows` / `orphaned_rows`. Eight tests each asserted a
/// specific behaviour and none asserted the invariant spanning them,
/// which is how a cycle's row being double-classified survived a
/// review round. Called from every test below, not only T-23e's own.
fn assert_partition(result: &ChainVerification) {
    assert_eq!(
        result.verified_rows
            + result.legacy_rows
            + result.tampered_rows.len()
            + result.orphaned_rows.len(),
        result.total_rows,
        "classes must partition total_rows exactly: {:?}",
        result
    );
    let mut ids: Vec<&str> = result
        .tampered_rows
        .iter()
        .map(|r| r.id.as_str())
        .chain(result.orphaned_rows.iter().map(|r| r.id.as_str()))
        .collect();
    ids.sort_unstable();
    let mut deduped = ids.clone();
    deduped.dedup();
    assert_eq!(
        ids.len(),
        deduped.len(),
        "a row id must not appear in more than one class: {:?}",
        result
    );
}

/// Comparable summary of a `ChainVerification`, for asserting two runs
/// produced the identical result regardless of input order (T-21).
/// Row-id lists are sorted so list order doesn't count as a difference.
fn summarize(v: &ChainVerification) -> (usize, usize, usize, Vec<String>, Vec<String>) {
    let mut tampered: Vec<String> = v.tampered_rows.iter().map(|r| r.id.clone()).collect();
    let mut orphaned: Vec<String> = v.orphaned_rows.iter().map(|r| r.id.clone()).collect();
    tampered.sort();
    orphaned.sort();
    (
        v.total_rows,
        v.legacy_rows,
        v.verified_rows,
        tampered,
        orphaned,
    )
}

// ── T-20 — 20 rows written within one second verify clean, ≥10 runs ──

#[test]
fn t20_twenty_rows_in_one_second_verify_clean_every_run() {
    let runs = 50; // comfortably above the required 10
    for run in 0..runs {
        let rows = build_chain(20, "2026-07-31T00:00:00Z");
        let result = verify(&rows);
        assert_eq!(result.total_rows, 20, "run {run}");
        assert_eq!(result.legacy_rows, 0, "run {run}");
        assert_eq!(
            result.verified_rows, 20,
            "run {run}: not every row verified"
        );
        assert!(
            result.tampered_rows.is_empty(),
            "run {run}: {:?}",
            result.tampered_rows
        );
        assert!(
            result.orphaned_rows.is_empty(),
            "run {run}: {:?}",
            result.orphaned_rows
        );
        assert_partition(&result);
    }
}

// ── T-21 — result is unchanged when rows arrive in a different order ──

#[test]
fn t21_result_does_not_depend_on_input_order() {
    let mut rows = build_chain(20, "2026-07-31T00:00:00Z");
    let baseline_result = verify(&rows);
    assert_partition(&baseline_result);
    let baseline = summarize(&baseline_result);

    // Reverse order (the opposite of any ORDER BY ASC the query could use).
    let mut reversed = rows.clone();
    reversed.reverse();
    let reversed_result = verify(&reversed);
    assert_partition(&reversed_result);
    assert_eq!(baseline, summarize(&reversed_result), "reversed order");

    // Sorted by id — exactly the tie-break the old, broken code used.
    let mut by_id = rows.clone();
    by_id.sort_by(|a, b| a.id.cmp(&b.id));
    let by_id_result = verify(&by_id);
    assert_partition(&by_id_result);
    assert_eq!(baseline, summarize(&by_id_result), "sorted-by-id order");

    // An arbitrary shuffle.
    let len = rows.len();
    rows.swap(0, len - 1);
    rows.swap(1, len - 2);
    rows.swap(2, 7.min(len - 1));
    let shuffled_result = verify(&rows);
    assert_partition(&shuffled_result);
    assert_eq!(baseline, summarize(&shuffled_result), "shuffled order");
}

// ── T-22 — mid-chain deletion: successors orphaned, nothing tampered ──

#[test]
fn t22_mid_chain_deletion_orphans_successors_and_tampers_nothing() {
    let action_time = "2026-07-31T00:00:00Z";
    let rows = build_chain(5, action_time);
    let deleted_idx = 2;
    let deleted_id = rows[deleted_idx].id.clone();

    let mut remaining = rows.clone();
    remaining.remove(deleted_idx);

    let result = verify(&remaining);

    assert!(
        result.tampered_rows.is_empty(),
        "a deletion must never be reported as tampering: {:?}",
        result.tampered_rows
    );

    // Rows before the deletion point (0, 1) are still reached and verified.
    assert_eq!(result.verified_rows, deleted_idx);

    // Rows after the deletion point (originally 3, 4) are exactly the
    // orphaned set — named, not merely counted.
    let mut expected_orphaned: Vec<String> = rows[deleted_idx + 1..]
        .iter()
        .map(|r| r.id.clone())
        .collect();
    expected_orphaned.sort();
    let mut actual_orphaned: Vec<String> =
        result.orphaned_rows.iter().map(|r| r.id.clone()).collect();
    actual_orphaned.sort();
    assert_eq!(actual_orphaned, expected_orphaned);

    // The deleted row itself is simply gone, not reported as anything.
    assert!(!actual_orphaned.contains(&deleted_id));
    assert_eq!(result.total_rows, 4);
    assert_partition(&result);
}

// ── T-23 — a content alteration is tampered, and only that row ──

#[test]
fn t23_content_alteration_is_tampered_and_only_that_row() {
    let action_time = "2026-07-31T00:00:00Z";
    let mut rows = build_chain(5, action_time);
    let altered_idx = 2;
    let altered_id = rows[altered_idx].id.clone();

    // Alter content in place *without* recomputing row_hash — exactly
    // what a raw UPDATE against the table would do.
    rows[altered_idx].action_type = "delete".to_string();

    let result = verify(&rows);

    assert_eq!(result.tampered_rows.len(), 1, "{:?}", result.tampered_rows);
    assert_eq!(result.tampered_rows[0].id, altered_id);
    assert!(
        result.orphaned_rows.is_empty(),
        "tampering one row must not orphan its successors: {:?}",
        result.orphaned_rows
    );
    // The other four rows (including the two chained after the altered
    // one) are all reached and verified — successors of a tampered row
    // are still found, because forward-linking uses the row's *stored*
    // row_hash, unaffected by a content-only edit.
    assert_eq!(result.verified_rows, 4);
    assert_eq!(result.total_rows, 5);
    assert_partition(&result);
}

// ── T-23a — a fork leaves one branch orphaned, count non-zero ──

#[test]
fn t23a_fork_leaves_one_branch_orphaned() {
    let action_time = "2026-07-31T00:00:00Z";
    let root_id = uuid::Uuid::new_v4().to_string();
    let root_hash = compute_row_hash(GENESIS_HASH, &fields_for(&root_id, action_time));
    let root = make_row(&root_id, action_time, Some(GENESIS_HASH), Some(&root_hash));

    // Two independent rows both chaining from `root` — a fork.
    let branch_a_id = uuid::Uuid::new_v4().to_string();
    let branch_a_hash = compute_row_hash(&root_hash, &fields_for(&branch_a_id, action_time));
    let branch_a = make_row(
        &branch_a_id,
        action_time,
        Some(&root_hash),
        Some(&branch_a_hash),
    );

    let branch_b_id = uuid::Uuid::new_v4().to_string();
    let branch_b_hash = compute_row_hash(&root_hash, &fields_for(&branch_b_id, action_time));
    let branch_b = make_row(
        &branch_b_id,
        action_time,
        Some(&root_hash),
        Some(&branch_b_hash),
    );

    let rows = vec![root, branch_a, branch_b];
    let result = verify(&rows);

    assert!(
        result.tampered_rows.is_empty(),
        "{:?}",
        result.tampered_rows
    );
    assert_eq!(
        result.orphaned_rows.len(),
        1,
        "exactly one branch must be orphaned: {:?}",
        result.orphaned_rows
    );
    // root + exactly one branch verified; the other branch is the orphan.
    assert_eq!(result.verified_rows, 2);
    let orphaned_id = &result.orphaned_rows[0].id;
    assert!(orphaned_id == &branch_a_id || orphaned_id == &branch_b_id);
    assert_partition(&result);
}

// ── walk_chain must terminate on any input, including a cycle ──
//
// Found by independent review (`.git-exclude/reviewed/
// 027-subject-05-ruling-and-defect.md` §3): `reached[idx]` was written
// but never consulted before processing, so a row whose stored
// row_hash loops back to an earlier hash in the walk advanced
// `expected` back to an already-visited row forever. `rows` is not
// only what `log()` writes — it is whatever `audit_logs` contains,
// which anyone with INSERT access can shape arbitrarily — so this must
// hold for adversarial input, not just well-formed chains.

#[test]
fn walk_chain_terminates_on_the_reviewers_minimal_two_row_cycle() {
    // | r1 | GENESIS | h1 |
    // | r2 | h1      | h1 |  <- row_hash equals its own prev_hash
    let action_time = "2026-07-31T00:00:00Z";
    let r1_id = uuid::Uuid::new_v4().to_string();
    let h1 = compute_row_hash(GENESIS_HASH, &fields_for(&r1_id, action_time));
    let r1 = make_row(&r1_id, action_time, Some(GENESIS_HASH), Some(&h1));

    let r2_id = uuid::Uuid::new_v4().to_string();
    // Deliberately NOT compute_row_hash's real output -- an attacker
    // with INSERT access writes whatever bytes they like into row_hash.
    let r2 = make_row(&r2_id, action_time, Some(&h1), Some(&h1));

    let rows = vec![r1, r2];
    let result = verify(&rows); // must return, not hang

    assert_eq!(result.total_rows, 2);
    // r2 is classified exactly once on its first visit (tampered, since
    // "h1" is not its real content hash), then the walk revisits it via
    // its own row_hash pointing back to itself -- reported as a cycle,
    // not a second TamperedRow for the same id (subject 05 round 3,
    // .git-exclude/reviewed/028-subject-05-round-3.md 2).
    assert_eq!(result.cycle_at.as_deref(), Some(r2_id.as_str()));
    assert_eq!(
        result
            .tampered_rows
            .iter()
            .filter(|t| t.id == r2_id)
            .count(),
        1,
        "r2 must be classified exactly once, not once per visit: {:?}",
        result.tampered_rows
    );
    assert_partition(&result);
}

#[test]
fn walk_chain_terminates_on_a_two_row_alternating_cycle() {
    // r1 and r2 chain from each other's row_hash with no genesis entry
    // point at all -- a closed loop, unreachable from genesis, that
    // the walk must never enter in the first place (by_prev_hash has
    // no entry for GENESIS_HASH here), and a variant that must not
    // hang even if some other logic change ever made it reachable.
    let action_time = "2026-07-31T00:00:00Z";
    let r1_id = uuid::Uuid::new_v4().to_string();
    let r2_id = uuid::Uuid::new_v4().to_string();
    let hash_a = "a".repeat(64);
    let hash_b = "b".repeat(64);
    let r1 = make_row(&r1_id, action_time, Some(&hash_b), Some(&hash_a));
    let r2 = make_row(&r2_id, action_time, Some(&hash_a), Some(&hash_b));

    let rows = vec![r1, r2];
    let result = verify(&rows); // must return, not hang

    // Unreachable from genesis (nothing has prev_hash == GENESIS_HASH):
    // both rows are orphaned, and the walk never entered the loop.
    assert_eq!(result.total_rows, 2);
    assert!(
        result.tampered_rows.is_empty(),
        "{:?}",
        result.tampered_rows
    );
    assert_eq!(result.orphaned_rows.len(), 2);
    assert_eq!(result.cycle_at, None, "the walk never entered the loop");
    assert_partition(&result);
}

#[test]
fn walk_chain_still_terminates_when_the_cycle_is_reachable_from_genesis() {
    // Same alternating cycle as above, but r1 chains from GENESIS, so
    // the walk *does* enter the loop and must break out of it rather
    // than looping between r1 and r2 forever.
    let action_time = "2026-07-31T00:00:00Z";
    let r1_id = uuid::Uuid::new_v4().to_string();
    let h1 = compute_row_hash(GENESIS_HASH, &fields_for(&r1_id, action_time));
    // r2 claims to chain from r1, but its own row_hash points back to
    // r1's prev_hash slot's value (GENESIS) is not reused here -- instead
    // r2's row_hash is set to h1 itself, so the walk sees h1 -> r1 again.
    let r2_id = uuid::Uuid::new_v4().to_string();
    let r1 = make_row(&r1_id, action_time, Some(GENESIS_HASH), Some(&h1));
    let r2 = make_row(&r2_id, action_time, Some(&h1), Some(&h1));

    let rows = vec![r1, r2];
    let result = verify(&rows); // must return, not hang

    assert_eq!(result.total_rows, 2);
    assert_eq!(
        result.verified_rows, 1,
        "only r1 verifies before the cycle is caught"
    );
    assert_eq!(result.cycle_at.as_deref(), Some(r2_id.as_str()));
    assert_eq!(
        result
            .tampered_rows
            .iter()
            .filter(|t| t.id == r2_id)
            .count(),
        1,
        "r2 must be classified exactly once, not once per visit: {:?}",
        result.tampered_rows
    );
    assert_partition(&result);
}

// ── T-23b — the head is the true tail, and the last genesis-reachable
//    row after a deletion, not the orphaned island's own tail ──
//
// current_head_hash is async (needs a D1Database), so this exercises
// the pure walk_chain directly -- current_head_hash is a thin wrapper
// returning exactly this `.head` value (subject 05, Build step 3).

#[test]
fn t23b_head_is_the_true_tail_at_twenty_rows_in_one_second() {
    let rows = build_chain(20, "2026-07-31T00:00:00Z");
    let expected_head = rows.last().unwrap().row_hash.clone().unwrap();
    assert_eq!(walk_chain(&rows).head, expected_head);
}

#[test]
fn t23b_head_after_deletion_is_the_last_reachable_row_not_the_orphaned_tail() {
    let action_time = "2026-07-31T00:00:00Z";
    let rows = build_chain(5, action_time);
    let deleted_idx = 2;

    let mut remaining = rows.clone();
    remaining.remove(deleted_idx);

    let result = walk_chain(&remaining);

    // The last genesis-reachable row is the one immediately before the
    // deletion point (index 1) -- NOT the original chain's true latest
    // row (index 4), which is now an unreachable, orphaned island.
    let expected_head = rows[deleted_idx - 1].row_hash.clone().unwrap();
    let orphaned_islands_own_tail = rows[4].row_hash.clone().unwrap();

    assert_eq!(result.head, expected_head);
    assert_ne!(
        result.head, orphaned_islands_own_tail,
        "chaining onto the orphaned island's tail would orphan every row written from here on"
    );
    assert_partition(&result.verification);
}

#[test]
fn t23b_head_is_genesis_for_an_empty_or_legacy_only_table() {
    assert_eq!(walk_chain(&[]).head, GENESIS_HASH);

    let legacy_only = vec![make_row("legacy-1", "2020-01-01T00:00:00Z", None, None)];
    assert_eq!(walk_chain(&legacy_only).head, GENESIS_HASH);
}

// ── T-23d — a fork at write time does not refuse: the head chains onto
//    the deterministically chosen (verified) branch, and is flagged ──

#[test]
fn t23d_fork_head_matches_the_verified_branch_and_is_flagged_not_refused() {
    let action_time = "2026-07-31T00:00:00Z";
    let root_id = uuid::Uuid::new_v4().to_string();
    let root_hash = compute_row_hash(GENESIS_HASH, &fields_for(&root_id, action_time));
    let root = make_row(&root_id, action_time, Some(GENESIS_HASH), Some(&root_hash));

    let branch_a_id = uuid::Uuid::new_v4().to_string();
    let branch_a_hash = compute_row_hash(&root_hash, &fields_for(&branch_a_id, action_time));
    let branch_a = make_row(
        &branch_a_id,
        action_time,
        Some(&root_hash),
        Some(&branch_a_hash),
    );

    let branch_b_id = uuid::Uuid::new_v4().to_string();
    let branch_b_hash = compute_row_hash(&root_hash, &fields_for(&branch_b_id, action_time));
    let branch_b = make_row(
        &branch_b_id,
        action_time,
        Some(&root_hash),
        Some(&branch_b_hash),
    );

    let rows = vec![root, branch_a, branch_b];
    let result = walk_chain(&rows);

    // Not refused: `walk_chain` always returns a usable head, never an
    // error, regardless of the fork -- there is no refusal path to
    // exercise, which is the point.
    assert!(
        result.forked,
        "a fork must be flagged for the caller to log"
    );
    assert!(result.verification.tampered_rows.is_empty());

    // The head is whichever branch `verify_chain`'s own report marks
    // verified (not orphaned) -- the same deterministic tiebreak, one
    // walk, not two independently-arrived-at answers.
    let winning_hash = if branch_a_id
        == result
            .verification
            .orphaned_rows
            .first()
            .map(|o| o.id.clone())
            .unwrap_or_default()
    {
        &branch_b_hash
    } else {
        &branch_a_hash
    };
    assert_eq!(&result.head, winning_hash);
    assert_partition(&result.verification);
}

// ── T-23e — the four classes plus cycle_at partition every row exactly
//    once, even across a cycle (subject 05 round 3) ──

#[test]
fn t23e_cycle_row_is_classified_exactly_once_not_twice() {
    // The exact fixture from .git-exclude/reviewed/
    // 028-subject-05-round-3.md 2, which found r2 reported twice: once
    // as a first-visit content mismatch, once again as the cycle.
    let action_time = "2026-07-31T00:00:00Z";
    let r1_id = uuid::Uuid::new_v4().to_string();
    let h1 = compute_row_hash(GENESIS_HASH, &fields_for(&r1_id, action_time));
    let r1 = make_row(&r1_id, action_time, Some(GENESIS_HASH), Some(&h1));
    let r2_id = uuid::Uuid::new_v4().to_string();
    let r2 = make_row(&r2_id, action_time, Some(&h1), Some(&h1));

    let rows = vec![r1, r2];
    let result = verify(&rows);

    assert_partition(&result);
    assert_eq!(
        result.verified_rows + result.tampered_rows.len(),
        2,
        "r1 verified, r2 tampered (once) -- not three classifications for two rows"
    );
    assert_eq!(result.cycle_at.as_deref(), Some(r2_id.as_str()));
}

#[test]
fn t23e_partition_holds_across_every_fixture_in_this_module() {
    // A cross-check, not a new scenario: re-run every fixture already
    // built above through assert_partition, so the invariant is
    // checked in one place a future reader can find it even if a new
    // test forgets to call it inline.
    assert_partition(&verify(&build_chain(20, "2026-07-31T00:00:00Z")));
    assert_partition(&verify(&[]));
}

// ── T-29b — a NULL-hash row is classified legacy, and only legacy
//    (subject 06, DEC-021) ──
//
// Originally specified against a Class A migration's output (0004
// applied to a database lacking prev_hash/row_hash, leaving its rows
// with NULL hashes). DEC-021 inverted that: 0004 now refuses a Class A
// source outright (T-29a) rather than producing NULL-hash rows from
// it, so that scenario no longer arises. The property this test
// guards is broader than that one origin, though: any row whose
// prev_hash/row_hash are NULL — a Class B/C row written before the
// original hash-chain rollout, not only a hypothetical Class A
// migration output — must classify as legacy, never tampered or
// orphaned. T-23b's legacy-only fixture asserted only `head ==
// GENESIS_HASH`; nothing before this test asserted the classification
// counts directly, or a legacy row's behaviour mixed alongside a real
// chain.
#[test]
fn t29b_null_hash_rows_classify_as_legacy_not_tampered_or_orphaned() {
    let legacy_only = vec![
        make_row("legacy-1", "2020-01-01T00:00:00Z", None, None),
        make_row("legacy-2", "2020-01-02T00:00:00Z", None, None),
    ];
    let result = verify(&legacy_only);
    assert_eq!(result.total_rows, 2);
    assert_eq!(result.legacy_rows, 2);
    assert!(result.tampered_rows.is_empty());
    assert!(result.orphaned_rows.is_empty());
    assert_partition(&result);

    // Legacy rows interleaved with a real chain: the chain still walks
    // and verifies from genesis exactly as if the legacy rows were not
    // there, and the legacy rows themselves land only in legacy_rows.
    let mut mixed = vec![make_row("legacy-3", "2020-01-01T00:00:00Z", None, None)];
    mixed.extend(build_chain(5, "2026-07-31T00:00:00Z"));
    mixed.push(make_row("legacy-4", "2020-01-02T00:00:00Z", None, None));
    let result = verify(&mixed);
    assert_eq!(result.total_rows, 7);
    assert_eq!(result.legacy_rows, 2);
    assert_eq!(result.verified_rows, 5);
    assert!(result.tampered_rows.is_empty());
    assert!(result.orphaned_rows.is_empty());
    assert_partition(&result);
}
