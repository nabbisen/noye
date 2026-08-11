//! Tests for `db/migration.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md`.
//!
//! Everything here is a source scan, not a D1-backed test — this project
//! has no Miniflare/Wrangler test harness (no test here can construct a
//! `D1Database`), so the async upsert/import functions are verified
//! against real local D1 via `wrangler dev --local`, evidence captured
//! under `.git-exclude/evidence/`, the same discipline used for every
//! other D1-touching module in this crate.

const SOURCE: &str = include_str!("../migration.rs");

// ── T-45 / T-45a (subject 09, G-22): no `INSERT OR REPLACE` remains ──

#[test]
fn no_insert_or_replace_remains() {
    // The exact SQL shape, not the phrase alone -- this module's own doc
    // comments mention "INSERT OR REPLACE" in prose explaining why it was
    // removed, which must not trip this check.
    assert!(
        !SOURCE.contains("INSERT OR REPLACE INTO"),
        "INSERT OR REPLACE reintroduces G-22's cascade-deletion defect \
         (ON DELETE CASCADE fires on the delete-then-insert REPLACE does) \
         -- use an explicit ON CONFLICT(...) DO UPDATE SET upsert instead"
    );
}

#[test]
fn every_upsert_uses_on_conflict_do_update() {
    // All five tables subject 09 converted: targets, notification_channels,
    // maintenance_windows, users, target_notifications.
    let on_conflict_count = SOURCE.matches("ON CONFLICT(").count();
    assert_eq!(
        on_conflict_count, 5,
        "expected exactly 5 ON CONFLICT upserts (one per converted table), found {}",
        on_conflict_count
    );
}

// ── T-51a (subject 10, G-38/G-39 boundary): the new threshold fields
// bind through i64_to_d1, never a raw JsValue::from cast ──

#[test]
fn threshold_fields_route_through_i64_to_d1() {
    assert!(
        SOURCE.contains("i64_to_d1(t.success_threshold)"),
        "success_threshold must bind through i64_to_d1, not a raw JsValue::from \
         (i64_to_d1 rejects rather than truncates outside +/-2^53 -- G-38/G-39)"
    );
    assert!(
        SOURCE.contains("i64_to_d1(t.failure_threshold)"),
        "failure_threshold must bind through i64_to_d1, not a raw JsValue::from"
    );
}

#[test]
fn no_raw_jsvalue_cast_of_threshold_fields() {
    assert!(!SOURCE.contains("JsValue::from(t.success_threshold)"));
    assert!(!SOURCE.contains("JsValue::from(t.failure_threshold)"));
}
