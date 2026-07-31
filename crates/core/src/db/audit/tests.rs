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
        let result = walk_chain(&rows);
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
    }
}

// ── T-21 — result is unchanged when rows arrive in a different order ──

#[test]
fn t21_result_does_not_depend_on_input_order() {
    let mut rows = build_chain(20, "2026-07-31T00:00:00Z");
    let baseline = summarize(&walk_chain(&rows));

    // Reverse order (the opposite of any ORDER BY ASC the query could use).
    let mut reversed = rows.clone();
    reversed.reverse();
    assert_eq!(
        baseline,
        summarize(&walk_chain(&reversed)),
        "reversed order"
    );

    // Sorted by id — exactly the tie-break the old, broken code used.
    let mut by_id = rows.clone();
    by_id.sort_by(|a, b| a.id.cmp(&b.id));
    assert_eq!(
        baseline,
        summarize(&walk_chain(&by_id)),
        "sorted-by-id order"
    );

    // An arbitrary shuffle.
    let len = rows.len();
    rows.swap(0, len - 1);
    rows.swap(1, len - 2);
    rows.swap(2, 7.min(len - 1));
    assert_eq!(baseline, summarize(&walk_chain(&rows)), "shuffled order");
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

    let result = walk_chain(&remaining);

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

    let result = walk_chain(&rows);

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
    let result = walk_chain(&rows);

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
}
