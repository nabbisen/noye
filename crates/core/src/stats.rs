//! Pure SLA / availability calculator.
//!
//! Given a list of incidents and (optionally) maintenance windows that overlap
//! a time window, compute downtime and uptime ratios. This module is pure: no
//! D1, no Worker types. The thin wrapper that fetches inputs from D1 lives in
//! `api::stats`.
//!
//! ## Methodology
//!
//! - **Window.** Reports are over a half-open time interval `[start, end)`.
//! - **Downtime.** The sum of seconds spent in the `down` state during the
//!   window. We compute this by clipping each incident's `[opened_at,
//!   resolved_at)` interval to the window and summing the lengths. Open
//!   incidents (no `resolved_at`) are clipped against the window's `end`.
//! - **Maintenance overlap.** Maintenance windows that overlap the report
//!   window are *also* clipped, then their overlap with downtime intervals is
//!   subtracted from the downtime total to produce SLA-adjusted downtime.
//!   This means: a 1-hour outage that happened entirely during a maintenance
//!   window contributes 0 to the SLA number but still counts in gross
//!   uptime.
//! - **MTTR.** Mean time to recovery, averaged only across *resolved*
//!   incidents that overlap the window. Open incidents are excluded — their
//!   recovery time is not yet known.
//!
//! ## Limits
//!
//! - The calculator does not account for the period before the system started
//!   monitoring a target. If a target was created mid-window, the report will
//!   over-credit it for "uptime" during the period when no checks were
//!   running. Callers should set `window_start` no earlier than the target's
//!   `created_at` to avoid this.
//! - Overlapping incidents (which should not exist in practice — the
//!   state-transition rules prevent it) are deduped by union, not summed, so
//!   the downtime number stays consistent.

use chrono::{DateTime, Utc};
use noye_shared::{Incident, MaintenanceWindow, SlaReport};

/// Internal representation of a half-open `[start, end)` time range in
/// seconds-since-epoch. We work in seconds because all our timestamps are
/// already at second resolution and arithmetic on i64 avoids any floating
/// point drift.
#[derive(Debug, Clone, Copy)]
struct Range {
    start: i64,
    end: i64,
}

impl Range {
    fn len(&self) -> i64 {
        (self.end - self.start).max(0)
    }
    fn intersect(&self, other: &Range) -> Option<Range> {
        let s = self.start.max(other.start);
        let e = self.end.min(other.end);
        if s < e { Some(Range { start: s, end: e }) } else { None }
    }
}

/// Parse an ISO-8601 UTC timestamp. We accept both `Z` and `+00:00` suffixes
/// and the fractional-second variants that chrono emits.
fn parse(ts: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|| {
            // SQLite's `datetime('now')` produces "YYYY-MM-DD HH:MM:SS" without
            // timezone or `T`; we standardize on the with-`T` Z form, but be
            // defensive in case some legacy rows still use the SQLite format.
            chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%dT%H:%M:%SZ")
                .or_else(|_| chrono::NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M:%S"))
                .ok()
                .map(|n| n.and_utc())
        })
}

/// Convert a list of incidents into clipped ranges within the report window.
fn incident_ranges(incidents: &[&Incident], window: &Range) -> Vec<Range> {
    incidents
        .iter()
        .filter_map(|inc| {
            let start = parse(&inc.opened_at)?.timestamp();
            let end = match inc.resolved_at.as_deref().and_then(parse) {
                Some(dt) => dt.timestamp(),
                None => window.end, // Open incidents extend to the window's end.
            };
            Range { start, end }.intersect(window)
        })
        .collect()
}

fn maintenance_ranges(windows: &[&MaintenanceWindow], window: &Range) -> Vec<Range> {
    windows
        .iter()
        .filter_map(|m| {
            let start = parse(&m.start_at)?.timestamp();
            let end = parse(&m.end_at)?.timestamp();
            Range { start, end }.intersect(window)
        })
        .collect()
}

/// Merge overlapping ranges into a disjoint set, preserving total covered
/// time. Used to ensure overlapping incidents don't double-count.
fn union(mut ranges: Vec<Range>) -> Vec<Range> {
    if ranges.is_empty() {
        return ranges;
    }
    ranges.sort_by_key(|r| r.start);
    let mut out: Vec<Range> = Vec::with_capacity(ranges.len());
    for r in ranges {
        match out.last_mut() {
            Some(last) if r.start <= last.end => {
                if r.end > last.end {
                    last.end = r.end;
                }
            }
            _ => out.push(r),
        }
    }
    out
}

/// Total covered seconds in a disjoint set of ranges.
fn total_seconds(ranges: &[Range]) -> i64 {
    ranges.iter().map(Range::len).sum()
}

/// Subtract one disjoint set of ranges from another. Both inputs must be
/// disjoint and sorted (the output of `union`).
fn subtract(minuend: &[Range], subtrahend: &[Range]) -> Vec<Range> {
    let mut result = Vec::new();
    for &m in minuend {
        let mut current_start = m.start;
        for &s in subtrahend {
            if s.end <= current_start {
                continue;
            }
            if s.start >= m.end {
                break;
            }
            if s.start > current_start {
                result.push(Range { start: current_start, end: s.start.min(m.end) });
            }
            current_start = s.end.max(current_start);
            if current_start >= m.end {
                break;
            }
        }
        if current_start < m.end {
            result.push(Range { start: current_start, end: m.end });
        }
    }
    result
}

/// Inputs to the report computation. Bundled so the public function signature
/// stays manageable as we add more fields.
pub struct SlaInputs<'a> {
    pub target_id: &'a str,
    pub target_name: &'a str,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    /// Every incident on the target whose `[opened_at, resolved_at)` overlaps
    /// the window. Filtering to overlapping rows is the caller's job (a SQL
    /// query is the most efficient way).
    pub incidents: &'a [&'a Incident],
    /// Maintenance windows applicable to this target that overlap the window.
    /// The caller is responsible for the applicability rules (matching
    /// `target_id`, `target_tag`, or global).
    pub maintenance: &'a [&'a MaintenanceWindow],
}

/// Compute the SLA report. Pure function; safe to test and reason about
/// independently of any I/O layer.
pub fn compute_sla(inputs: SlaInputs<'_>) -> SlaReport {
    let window = Range {
        start: inputs.window_start.timestamp(),
        end: inputs.window_end.timestamp(),
    };
    let window_seconds = window.len();

    let incident_set = union(incident_ranges(inputs.incidents, &window));
    let maintenance_set = union(maintenance_ranges(inputs.maintenance, &window));

    let downtime_seconds = total_seconds(&incident_set);

    // SLA-adjusted downtime: subtract maintenance overlap from incident set,
    // *then* sum. This is the right order — we don't want to count a
    // maintenance period that didn't actually have an outage.
    let sla_downtime_seconds = total_seconds(&subtract(&incident_set, &maintenance_set));
    let maintenance_seconds = total_seconds(&maintenance_set);

    let gross_uptime_ratio = if window_seconds > 0 {
        ((window_seconds - downtime_seconds) as f64 / window_seconds as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let sla_uptime_ratio = if window_seconds > 0 {
        ((window_seconds - sla_downtime_seconds) as f64 / window_seconds as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let resolved_durations: Vec<i64> = inputs
        .incidents
        .iter()
        .filter(|inc| inc.resolved_at.is_some())
        .filter_map(|inc| inc.duration_sec)
        .collect();
    let mttr_seconds = if resolved_durations.is_empty() {
        None
    } else {
        Some(resolved_durations.iter().sum::<i64>() / resolved_durations.len() as i64)
    };

    SlaReport {
        target_id: inputs.target_id.to_string(),
        target_name: inputs.target_name.to_string(),
        window_start: inputs.window_start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        window_end: inputs.window_end.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        window_seconds,
        downtime_seconds,
        maintenance_seconds,
        gross_uptime_ratio,
        sla_uptime_ratio,
        incident_count: inputs.incidents.len() as i64,
        mttr_seconds,
    }
}

/// Convert a window string like `24h`, `7d`, `30d` into a duration of seconds.
/// Returns `None` for unrecognized formats. Used by the API layer to parse the
/// `?window=` query parameter.
pub fn parse_window(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (num_part, suffix) = s.split_at(s.len() - 1);
    let n: i64 = num_part.parse().ok()?;
    if n <= 0 {
        return None;
    }
    let multiplier = match suffix {
        "h" => 3600,
        "d" => 86_400,
        _ => return None,
    };
    Some(n * multiplier)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use noye_shared::{Incident, MaintenanceWindow};

    fn at(ymd_hms: (i32, u32, u32, u32, u32, u32)) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(ymd_hms.0, ymd_hms.1, ymd_hms.2, ymd_hms.3, ymd_hms.4, ymd_hms.5)
            .unwrap()
    }

    fn iso(dt: DateTime<Utc>) -> String {
        dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
    }

    fn incident(opened: DateTime<Utc>, resolved: Option<DateTime<Utc>>) -> Incident {
        let duration = resolved.map(|r| (r - opened).num_seconds());
        Incident {
            id: format!("inc-{}", opened.timestamp()),
            target_id: "t1".into(),
            status: if resolved.is_some() { "resolved".into() } else { "open".into() },
            opened_at: iso(opened),
            resolved_at: resolved.map(iso),
            duration_sec: duration,
            cause: None,
            resolution_note: None,
            created_by: None,
        }
    }

    fn maintenance(start: DateTime<Utc>, end: DateTime<Utc>) -> MaintenanceWindow {
        MaintenanceWindow {
            id: format!("m-{}", start.timestamp()),
            name: "scheduled".into(),
            start_at: iso(start),
            end_at: iso(end),
            target_tag: None,
            target_id: Some("t1".into()),
            suppress_notify: true,
            is_active: true,
            created_at: iso(start),
            created_by: "u1".into(),
            updated_by: "u1".into(),
        }
    }

    fn inputs<'a>(
        ws: DateTime<Utc>,
        we: DateTime<Utc>,
        incidents: &'a [&'a Incident],
        maintenance: &'a [&'a MaintenanceWindow],
    ) -> SlaInputs<'a> {
        SlaInputs {
            target_id: "t1",
            target_name: "API",
            window_start: ws,
            window_end: we,
            incidents,
            maintenance,
        }
    }

    // ── parse_window ──

    #[test]
    fn parse_window_recognizes_hours_and_days() {
        assert_eq!(parse_window("24h"), Some(86_400));
        assert_eq!(parse_window("1h"), Some(3_600));
        assert_eq!(parse_window("7d"), Some(7 * 86_400));
        assert_eq!(parse_window("30d"), Some(30 * 86_400));
    }

    #[test]
    fn parse_window_rejects_invalid_input() {
        assert_eq!(parse_window(""), None);
        assert_eq!(parse_window("24"), None);
        assert_eq!(parse_window("h"), None);
        assert_eq!(parse_window("0h"), None);
        assert_eq!(parse_window("-5h"), None);
        assert_eq!(parse_window("1m"), None); // minutes not yet supported
        assert_eq!(parse_window("foo"), None);
    }

    #[test]
    fn parse_window_trims_whitespace() {
        assert_eq!(parse_window("  24h  "), Some(86_400));
    }

    // ── compute_sla: zero-incident baseline ──

    #[test]
    fn no_incidents_yields_perfect_uptime() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let r = compute_sla(inputs(ws, we, &[], &[]));
        assert_eq!(r.window_seconds, 86_400);
        assert_eq!(r.downtime_seconds, 0);
        assert_eq!(r.gross_uptime_ratio, 1.0);
        assert_eq!(r.sla_uptime_ratio, 1.0);
        assert_eq!(r.incident_count, 0);
        assert_eq!(r.mttr_seconds, None);
    }

    // ── single incident ──

    #[test]
    fn one_resolved_incident_within_window() {
        // 10-minute outage in a 24-hour window
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let inc = incident(at((2026, 4, 1, 12, 0, 0)), Some(at((2026, 4, 1, 12, 10, 0))));
        let r = compute_sla(inputs(ws, we, &[&inc], &[]));
        assert_eq!(r.downtime_seconds, 600);
        assert!((r.gross_uptime_ratio - (86_400.0 - 600.0) / 86_400.0).abs() < 1e-9);
        assert_eq!(r.incident_count, 1);
        assert_eq!(r.mttr_seconds, Some(600));
    }

    #[test]
    fn open_incident_extends_to_window_end() {
        // Outage opened at hour 12 of a 24-hour window, still open at the
        // moment the report is run.
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let inc = incident(at((2026, 4, 1, 12, 0, 0)), None);
        let r = compute_sla(inputs(ws, we, &[&inc], &[]));
        assert_eq!(r.downtime_seconds, 12 * 3600);
        assert_eq!(r.mttr_seconds, None); // Open incident excluded from MTTR
    }

    #[test]
    fn incident_extending_past_window_end_is_clipped() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 1, 12, 0, 0));
        // Outage from 11:00 to 13:00; only the first hour is in the window
        let inc = incident(at((2026, 4, 1, 11, 0, 0)), Some(at((2026, 4, 1, 13, 0, 0))));
        let r = compute_sla(inputs(ws, we, &[&inc], &[]));
        assert_eq!(r.downtime_seconds, 3600);
    }

    #[test]
    fn incident_starting_before_window_is_clipped() {
        let ws = at((2026, 4, 1, 12, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        // Outage from 11:00 to 13:00; only the second hour is in the window
        let inc = incident(at((2026, 4, 1, 11, 0, 0)), Some(at((2026, 4, 1, 13, 0, 0))));
        let r = compute_sla(inputs(ws, we, &[&inc], &[]));
        assert_eq!(r.downtime_seconds, 3600);
    }

    #[test]
    fn incident_entirely_outside_window_contributes_nothing() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let inc = incident(at((2026, 3, 30, 0, 0, 0)), Some(at((2026, 3, 30, 1, 0, 0))));
        let r = compute_sla(inputs(ws, we, &[&inc], &[]));
        assert_eq!(r.downtime_seconds, 0);
        assert_eq!(r.gross_uptime_ratio, 1.0);
    }

    // ── overlapping incidents ──

    #[test]
    fn overlapping_incidents_are_unioned_not_summed() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let a = incident(at((2026, 4, 1, 10, 0, 0)), Some(at((2026, 4, 1, 11, 0, 0))));
        let b = incident(at((2026, 4, 1, 10, 30, 0)), Some(at((2026, 4, 1, 11, 30, 0))));
        let r = compute_sla(inputs(ws, we, &[&a, &b], &[]));
        // Union [10:00–11:00) ∪ [10:30–11:30) = [10:00–11:30) = 5400 seconds
        assert_eq!(r.downtime_seconds, 5400);
        assert_eq!(r.incident_count, 2);
    }

    #[test]
    fn adjacent_incidents_are_merged() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let a = incident(at((2026, 4, 1, 10, 0, 0)), Some(at((2026, 4, 1, 11, 0, 0))));
        let b = incident(at((2026, 4, 1, 11, 0, 0)), Some(at((2026, 4, 1, 12, 0, 0))));
        let r = compute_sla(inputs(ws, we, &[&a, &b], &[]));
        // Touching at 11:00 — merged into [10:00–12:00) = 7200 seconds
        assert_eq!(r.downtime_seconds, 7200);
    }

    // ── maintenance excludes from SLA ──

    #[test]
    fn outage_during_maintenance_does_not_count_against_sla() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let inc = incident(at((2026, 4, 1, 12, 0, 0)), Some(at((2026, 4, 1, 13, 0, 0))));
        let m = maintenance(at((2026, 4, 1, 11, 0, 0)), at((2026, 4, 1, 14, 0, 0)));
        let r = compute_sla(inputs(ws, we, &[&inc], &[&m]));

        // Gross uptime drops by the full outage hour
        assert_eq!(r.downtime_seconds, 3600);
        assert!((r.gross_uptime_ratio - (86_400.0 - 3600.0) / 86_400.0).abs() < 1e-9);
        // SLA uptime is perfect because the entire outage was inside the
        // maintenance window
        assert_eq!(r.sla_uptime_ratio, 1.0);
        assert_eq!(r.maintenance_seconds, 3 * 3600);
    }

    #[test]
    fn outage_partially_during_maintenance_partially_counts() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        // Outage 10:00–14:00 (4 hours)
        let inc = incident(at((2026, 4, 1, 10, 0, 0)), Some(at((2026, 4, 1, 14, 0, 0))));
        // Maintenance 12:00–16:00 — overlaps the latter half of the outage
        let m = maintenance(at((2026, 4, 1, 12, 0, 0)), at((2026, 4, 1, 16, 0, 0)));
        let r = compute_sla(inputs(ws, we, &[&inc], &[&m]));

        assert_eq!(r.downtime_seconds, 4 * 3600);
        // SLA-adjusted downtime is only the 10:00–12:00 part = 2 hours
        let sla_downtime_seconds = (1.0 - r.sla_uptime_ratio) * 86_400.0;
        assert!((sla_downtime_seconds - 2.0 * 3600.0).abs() < 1.0);
    }

    #[test]
    fn maintenance_with_no_overlapping_outage_has_no_sla_effect() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let m = maintenance(at((2026, 4, 1, 2, 0, 0)), at((2026, 4, 1, 4, 0, 0)));
        let r = compute_sla(inputs(ws, we, &[], &[&m]));
        // Window is otherwise clean, so both ratios are 1.0
        assert_eq!(r.gross_uptime_ratio, 1.0);
        assert_eq!(r.sla_uptime_ratio, 1.0);
        // Maintenance time is still reported for transparency
        assert_eq!(r.maintenance_seconds, 2 * 3600);
    }

    // ── MTTR ──

    #[test]
    fn mttr_averages_only_resolved_incidents() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let a = incident(at((2026, 4, 1, 1, 0, 0)), Some(at((2026, 4, 1, 1, 10, 0)))); // 600s
        let b = incident(at((2026, 4, 1, 5, 0, 0)), Some(at((2026, 4, 1, 5, 30, 0)))); // 1800s
        let c = incident(at((2026, 4, 1, 10, 0, 0)), None); // open, excluded
        let r = compute_sla(inputs(ws, we, &[&a, &b, &c], &[]));
        // MTTR = (600 + 1800) / 2 = 1200
        assert_eq!(r.mttr_seconds, Some(1200));
    }

    // ── numeric edge cases ──

    #[test]
    fn zero_length_window_returns_perfect_uptime() {
        let t = at((2026, 4, 1, 0, 0, 0));
        let r = compute_sla(inputs(t, t, &[], &[]));
        assert_eq!(r.window_seconds, 0);
        assert_eq!(r.gross_uptime_ratio, 1.0);
        assert_eq!(r.sla_uptime_ratio, 1.0);
    }

    #[test]
    fn malformed_timestamp_in_incident_is_ignored() {
        let ws = at((2026, 4, 1, 0, 0, 0));
        let we = at((2026, 4, 2, 0, 0, 0));
        let bad = Incident {
            id: "bad".into(),
            target_id: "t1".into(),
            status: "resolved".into(),
            opened_at: "garbage".into(),
            resolved_at: Some("also garbage".into()),
            duration_sec: Some(60),
            cause: None,
            resolution_note: None,
            created_by: None,
        };
        let r = compute_sla(inputs(ws, we, &[&bad], &[]));
        // The malformed incident is silently dropped from the downtime
        // calculation. It still counts toward `incident_count` because the
        // caller asked for it to be considered.
        assert_eq!(r.downtime_seconds, 0);
        assert_eq!(r.incident_count, 1);
    }

    #[test]
    fn parse_handles_both_iso_z_and_sqlite_format() {
        // Defensive: both formats should be acceptable since legacy rows
        // might use the SQLite default.
        assert!(parse("2026-04-01T12:00:00Z").is_some());
        assert!(parse("2026-04-01 12:00:00").is_some());
        assert!(parse("not-a-date").is_none());
    }
}
