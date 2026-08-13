//! Tests for `monitor/engine.rs`. Sibling module per PRQ-05.
//!
//! `retention_trigger` is the one piece of this module host-testable
//! without a Worker runtime -- everything else in `run_scheduled_checks`
//! touches `worker::Env`/`D1Database`/fetch. Subject 07g (G-43): pulled
//! out specifically so T-223/T-224 don't depend on controlling a
//! scheduled event's nominal time under `wrangler dev --local`, which
//! does not propagate one (confirmed against real `workerd`; see
//! `.git-exclude/reviewed/058-subject-07g-escalation-ruling.md`).

use super::*;

fn ms_at(iso: &str) -> f64 {
    chrono::DateTime::parse_from_rfc3339(iso)
        .unwrap()
        .timestamp_millis() as f64
}

// ── T-223 (must-fail-first against the pre-07g code, which read
// chrono::Utc::now() and could not be driven by an argument at all):
// nominal minute 00 -> Run ──

#[test]
fn t223_nominal_minute_00_runs() {
    assert_eq!(
        retention_trigger(ms_at("2026-01-01T00:00:00Z")),
        RetentionTrigger::Run
    );
}

#[test]
fn t223_nominal_minute_00_runs_regardless_of_hour() {
    // The defect this closes: an invocation nominally at, say, 13:00
    // that actually starts at 13:01 must still run, decided from the
    // nominal time -- hour is irrelevant to the decision.
    assert_eq!(
        retention_trigger(ms_at("2026-06-15T13:00:00Z")),
        RetentionTrigger::Run
    );
}

// ── T-224 (guard): nominal minute != 00 -> Skip, naming the minute ──

#[test]
fn t224_nominal_minute_37_skips_and_names_the_minute() {
    assert_eq!(
        retention_trigger(ms_at("2026-01-01T00:37:00Z")),
        RetentionTrigger::Skip {
            minute: "37".to_string()
        }
    );
}

#[test]
fn t224_nominal_minute_01_skips() {
    // The exact production shape from the handoff: a 00:00 invocation
    // that actually starts a minute late.
    assert_eq!(
        retention_trigger(ms_at("2026-01-01T00:01:00Z")),
        RetentionTrigger::Skip {
            minute: "01".to_string()
        }
    );
}

#[test]
fn t224_nominal_minute_59_skips() {
    assert_eq!(
        retention_trigger(ms_at("2026-01-01T00:59:00Z")),
        RetentionTrigger::Skip {
            minute: "59".to_string()
        }
    );
}

// ── UnreadableSchedule: a value outside chrono's representable range ──

#[test]
fn unrepresentable_timestamp_is_unreadable_not_a_silent_skip() {
    // f64 -> i64 casts saturate (Rust 1.45+); this lands far outside
    // chrono's ~262,000-year representable range either way.
    assert_eq!(
        retention_trigger(f64::MAX),
        RetentionTrigger::UnreadableSchedule
    );
    assert_eq!(
        retention_trigger(f64::MIN),
        RetentionTrigger::UnreadableSchedule
    );
}

#[test]
fn nan_does_not_silently_run_or_skip_as_minute_00() {
    // `f64::NAN as i64` saturates to 0 (1970-01-01T00:00:00Z, a real
    // minute-00 instant) under Rust's saturating float casts -- worth
    // pinning explicitly so a future Rust/chrono change that makes this
    // representable is caught rather than silently starting to run
    // retention on every NaN.
    assert_eq!(retention_trigger(f64::NAN), RetentionTrigger::Run);
}
