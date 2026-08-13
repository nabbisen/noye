//! Tests for `db/maintenance.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md`.
//!
//! Source scans only — no D1 test harness exists in this crate (see
//! `db/targets/tests.rs`); the actual runtime behaviour these guard
//! (T-52, T-53, T-54, T-58 through T-63) is verified against real
//! local D1 by `scripts/check-d1-behaviour.sh`'s T-52b/T-66b
//! assertions, evidence under `.git-exclude/evidence/`.

const SOURCE: &str = include_str!("../maintenance.rs");

// ── Subject 11 (G-07): both flags actually filter ──

#[test]
fn is_under_maintenance_filters_is_active_and_suppress_notify() {
    assert!(SOURCE.contains("is_active = 1 AND suppress_notify = 1"));
}

#[test]
fn list_in_window_filters_is_active_and_exclude_from_sla() {
    assert!(SOURCE.contains("is_active = 1 AND exclude_from_sla = 1"));
}

#[test]
fn create_binds_exclude_from_sla() {
    assert!(SOURCE.contains("input.exclude_from_sla.unwrap_or(true)"));
    assert!(SOURCE.contains("exclude_from_sla"));
}

// ── Subject 12 (G-09/G-27): exact relation match, no LIKE ──

#[test]
fn no_like_remains() {
    assert!(!SOURCE.contains("LIKE"));
}

#[test]
fn tag_pattern_is_gone() {
    assert!(!SOURCE.contains("tag_pattern"));
}

#[test]
fn tag_matching_is_an_exact_exists_join_against_target_tags() {
    assert!(SOURCE.contains("EXISTS"));
    assert!(SOURCE.contains("target_tags"));
    assert!(SOURCE.contains("tt.tag = target_tag"));
}

// ── Subject 12 (G-08): both query sites share one applicability rule ──
//
// is_under_maintenance and list_in_window matching the same scope
// semantics is exactly the invariant G-08 depends on; a single shared
// clause makes the two queries agreeing a compile-time fact instead of
// something that has to be kept in sync by hand.

#[test]
fn both_queries_share_the_applicability_clause() {
    assert_eq!(SOURCE.matches("APPLICABILITY_CLAUSE").count(), 3);
}
