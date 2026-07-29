//! Tests for `db/retention.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md` for the standing rule
//! against adding inline `#[cfg(test)] mod tests` blocks.

use super::*;
use chrono::TimeZone;

fn anchor() -> chrono::DateTime<chrono::Utc> {
    // 2026-04-01T00:00:00Z
    chrono::Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()
}

// ── compute_cutoff ──

#[test]
fn ninety_days_ago_at_midnight() {
    // 90 days before 2026-04-01 is 2026-01-01 (90 days = Jan 1 to Apr 1 in 2026, non-leap year)
    let cutoff = compute_cutoff(anchor(), 90);
    assert_eq!(cutoff, "2026-01-01T00:00:00Z");
}

#[test]
fn one_day_ago() {
    let cutoff = compute_cutoff(anchor(), 1);
    assert_eq!(cutoff, "2026-03-31T00:00:00Z");
}

#[test]
fn zero_days_ago_is_now() {
    let cutoff = compute_cutoff(anchor(), 0);
    assert_eq!(cutoff, "2026-04-01T00:00:00Z");
}

#[test]
fn cutoff_format_is_iso_8601_utc_z() {
    // Format must end with 'Z' (Zulu/UTC) and use 'T' as the date/time separator.
    // SQL comparisons against opened_at / action_time / checked_at depend on this.
    let cutoff = compute_cutoff(anchor(), 30);
    assert!(cutoff.ends_with('Z'));
    assert_eq!(cutoff.matches('T').count(), 1);
    assert_eq!(cutoff.len(), 20); // YYYY-MM-DDTHH:MM:SSZ
}

#[test]
fn cutoff_preserves_time_of_day() {
    let now = chrono::Utc
        .with_ymd_and_hms(2026, 4, 15, 13, 45, 30)
        .unwrap();
    let cutoff = compute_cutoff(now, 7);
    assert_eq!(cutoff, "2026-04-08T13:45:30Z");
}

// ── eligibility_where_clause (T-09, host-testable half) ──

#[test]
fn eligibility_known_for_every_default_retention_policy_table() {
    // sql/0001_initial.sql seeds retention_policies with exactly these
    // three table names; each must resolve.
    assert!(eligibility_where_clause("check_results").is_some());
    assert!(eligibility_where_clause("incidents").is_some());
    assert!(eligibility_where_clause("audit_logs").is_some());
}

#[test]
fn eligibility_unknown_for_an_unhandled_table() {
    // This is the case that used to silently fall through `_ => continue`
    // with no diagnostic (subject 02, T-09). The pure decision returns
    // None; run_cleanup is responsible for logging when it does.
    assert_eq!(eligibility_where_clause("not_a_real_table"), None);
    assert_eq!(eligibility_where_clause(""), None);
}

#[test]
fn incidents_eligibility_requires_resolved_status() {
    // Only resolved incidents are retention-eligible; open incidents must
    // never be swept regardless of age.
    let clause = eligibility_where_clause("incidents").unwrap();
    assert!(clause.contains("status = 'resolved'"));
}

#[test]
fn every_eligibility_clause_binds_cutoff_as_placeholder_one() {
    // Regression guard for the string-interpolated SQL this subject
    // removes: every clause must bind the cutoff, never embed the
    // caller's timestamp as a literal.
    for table in ["check_results", "incidents", "audit_logs"] {
        let clause = eligibility_where_clause(table).unwrap();
        assert!(
            clause.contains("?1"),
            "{table}'s eligibility clause does not bind ?1: {clause}"
        );
    }
}

// ── extract_ids ──

#[test]
fn extract_ids_reads_string_id_field_from_each_row() {
    let rows = vec![
        serde_json::json!({"id": "a", "other": 1}),
        serde_json::json!({"id": "b", "other": 2}),
    ];
    assert_eq!(extract_ids(&rows).unwrap(), vec!["a", "b"]);
}

#[test]
fn extract_ids_empty_batch_is_empty() {
    assert_eq!(extract_ids(&[]).unwrap(), Vec::<String>::new());
}

#[test]
fn extract_ids_reports_a_row_missing_id_rather_than_panicking() {
    let rows = vec![serde_json::json!({"no_id_here": true})];
    let err = extract_ids(&rows).unwrap_err();
    assert!(err.contains("missing string 'id' field"));
}

#[test]
fn extract_ids_reports_a_non_string_id_rather_than_coercing() {
    let rows = vec![serde_json::json!({"id": 12345})];
    assert!(extract_ids(&rows).is_err());
}

// ── requires_archival (T-09a, host-testable half) ──

#[test]
fn check_results_and_incidents_require_archival() {
    // DR-LIF-02 and DR-LIF-03: archive_to_r2 = 0 must not be honourable
    // for these classes.
    assert!(requires_archival("check_results"));
    assert!(requires_archival("incidents"));
}

#[test]
fn audit_logs_does_not_require_archival() {
    // No current requirement makes archival-before-deletion a
    // precondition for audit_logs; its retention-deletion behaviour is
    // handled independently in subject 04.
    assert!(!requires_archival("audit_logs"));
}

#[test]
fn an_unrecognized_table_does_not_require_archival() {
    // Irrelevant in practice — eligibility_where_clause already filters
    // unrecognized tables out before requires_archival is consulted —
    // but the function must not panic or default to `true` on an input
    // it was never designed for.
    assert!(!requires_archival("not_a_real_table"));
}
