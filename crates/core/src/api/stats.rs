//! Internal API for SLA / availability reports.
//!
//! Two routes:
//! - `GET /targets/:id/sla?window=24h` — single-target SLA report
//! - `GET /stats/sla?window=24h` — aggregate across all visible targets

use noye_shared::{SlaReport, SlaSummary};
use worker::*;

use crate::{api, db, stats};

/// Default window when the caller omits `?window=`. 24 hours mirrors the
/// dashboard's existing recency horizon and is cheap to compute.
const DEFAULT_WINDOW_SEC: i64 = 86_400;

/// Maximum window we'll honor in one request. SLA computation is linear in the
/// number of incidents in the window; for very long windows (a year of dense
/// incident data) the work could exceed Workers CPU limits. 90 days is well
/// beyond practical reporting needs.
const MAX_WINDOW_SEC: i64 = 90 * 86_400;

fn window_seconds_from_query(req: &Request) -> Result<i64> {
    let url = req.url()?;
    let raw = url.query_pairs().find(|(k, _)| k == "window").map(|(_, v)| v.to_string());
    let secs = match raw {
        None => DEFAULT_WINDOW_SEC,
        Some(ref s) => stats::parse_window(s)
            .ok_or_else(|| Error::RustError(format!("invalid window: {}", s)))?,
    };
    if secs > MAX_WINDOW_SEC {
        return Err(Error::RustError(format!(
            "window too large; max is {} days",
            MAX_WINDOW_SEC / 86_400
        )));
    }
    Ok(secs)
}

/// Single-target report.
pub async fn target_sla(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let window_sec = window_seconds_from_query(&req)?;

    let d = ctx.env.d1("DB")?;
    let report = build_report(&d, id, window_sec).await?;
    Response::from_json(&report)
}

/// Aggregate report across every visible target.
///
/// Members see only their own targets; admins see everything.
pub async fn aggregate_sla(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    let window_sec = window_seconds_from_query(&req)?;

    let d = ctx.env.d1("DB")?;
    let targets = db::targets::list_all(&d, &caller).await?;

    let mut per_target = Vec::with_capacity(targets.len());
    let mut total_window_seconds: i64 = 0;
    let mut total_downtime: i64 = 0;
    let mut total_sla_downtime: i64 = 0;

    for t in &targets {
        let report = build_report(&d, &t.id, window_sec).await?;
        total_window_seconds += report.window_seconds;
        total_downtime += report.downtime_seconds;
        // Recover SLA-adjusted downtime from the ratio (cleaner than carrying
        // an extra field on the public type).
        let sla_dt = ((1.0 - report.sla_uptime_ratio) * report.window_seconds as f64).round() as i64;
        total_sla_downtime += sla_dt;
        per_target.push(report);
    }

    let overall_gross_uptime_ratio = if total_window_seconds > 0 {
        ((total_window_seconds - total_downtime) as f64 / total_window_seconds as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let overall_sla_uptime_ratio = if total_window_seconds > 0 {
        ((total_window_seconds - total_sla_downtime) as f64 / total_window_seconds as f64).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let now = chrono::Utc::now();
    let window_start = now - chrono::Duration::seconds(window_sec);

    Response::from_json(&SlaSummary {
        window_start: window_start.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        window_end: now.format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        window_seconds: window_sec,
        per_target,
        overall_gross_uptime_ratio,
        overall_sla_uptime_ratio,
    })
}

/// Build a report for one target by fetching the windowed inputs from D1
/// and handing them to the pure `stats::compute_sla` calculator.
async fn build_report(d: &D1Database, target_id: &str, window_sec: i64) -> Result<SlaReport> {
    let now = chrono::Utc::now();
    let window_start = now - chrono::Duration::seconds(window_sec);
    let ws = window_start.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let we = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let target = db::targets::get_by_id(d, target_id).await?;
    let incidents = db::incidents::list_in_window(d, target_id, &ws, &we).await?;
    let maintenance =
        db::maintenance::list_in_window(d, target_id, target.tags.as_deref(), &ws, &we).await?;

    let inc_refs: Vec<&_> = incidents.iter().collect();
    let maint_refs: Vec<&_> = maintenance.iter().collect();

    Ok(stats::compute_sla(stats::SlaInputs {
        target_id,
        target_name: &target.name,
        window_start,
        window_end: now,
        incidents: &inc_refs,
        maintenance: &maint_refs,
    }))
}

/// Multi-window report for a single target.
///
/// Returns SLA reports for 24h, 7d, and 30d in one round-trip so the per-
/// target detail page doesn't need three separate Service Binding calls.
/// 90d is intentionally excluded — for the comparative-glance use case it
/// adds little signal beyond 30d, and computing it on every detail-page
/// load could push CPU budgets close to the limit on busy deployments.
pub async fn target_sla_multi(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;

    // Fetch the target name once so we can populate the response without
    // re-reading it from inside build_report's per-window call.
    let target = db::targets::get_by_id(&d, id).await?;

    let windows = [
        ("24h", 86_400_i64),
        ("7d", 7 * 86_400_i64),
        ("30d", 30 * 86_400_i64),
    ];

    let mut entries = Vec::with_capacity(windows.len());
    for (label, secs) in windows {
        let report = build_report(&d, id, secs).await?;
        entries.push(noye_shared::SlaWindowEntry {
            label: label.to_string(),
            report,
        });
    }

    Response::from_json(&noye_shared::SlaMultiReport {
        target_id: id.to_string(),
        target_name: target.name,
        windows: entries,
    })
}
