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

// ── Subject 12 (G-09/G-27): `targets.tags` is gone; every read goes
// through TARGET_COLUMNS's derived subquery, every write through
// set_tags -- a raw `SELECT *` or a `tags` bind here would either
// break (column no longer exists) or silently resurrect the dropped
// JSON column as a second, drifting source of truth ──

#[test]
fn no_select_star_from_targets() {
    assert!(!SOURCE.contains("SELECT * FROM targets"));
}

#[test]
fn create_and_update_route_tags_through_set_tags() {
    assert!(SOURCE.contains("set_tags(db, &id, input.tags.as_deref())"));
    assert!(SOURCE.contains("set_tags(db, id, input.tags.as_deref())"));
}
