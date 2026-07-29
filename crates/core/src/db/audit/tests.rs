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
