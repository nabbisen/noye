//! Tests for `db/targets.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md`.
//!
//! Source scans only — no D1 test harness exists in this crate (see
//! `db/migration/tests.rs`); `create`/`update`'s D1 behaviour is verified
//! against real local D1, evidence under `.git-exclude/evidence/`.

const SOURCE: &str = include_str!("../targets.rs");

// ── T-51a (subject 10, G-38/G-39 boundary): thresholds bind through
// i64_to_d1 in both create and update, never a raw JsValue::from cast ──

#[test]
fn create_and_update_route_thresholds_through_i64_to_d1() {
    assert!(SOURCE.contains("i64_to_d1(input.success_threshold.unwrap_or(3))"));
    assert!(SOURCE.contains("i64_to_d1(input.failure_threshold.unwrap_or(3))"));
    assert!(
        SOURCE.contains("i64_to_d1(input.success_threshold.unwrap_or(current.success_threshold))")
    );
    assert!(
        SOURCE.contains("i64_to_d1(input.failure_threshold.unwrap_or(current.failure_threshold))")
    );
}

#[test]
fn no_raw_jsvalue_cast_of_threshold_fields() {
    assert!(!SOURCE.contains("JsValue::from(input.success_threshold"));
    assert!(!SOURCE.contains("JsValue::from(input.failure_threshold"));
}
