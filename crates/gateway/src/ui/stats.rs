//! SSR for the SLA / availability pages.
//!
//! Two pages:
//! - `/stats` — aggregate across every visible target. `render_page`.
//! - `/stats/:id` — single target with multi-window comparison and the
//!   incident list for the selected window. `render_detail`.

use crate::ui::layout::{escape_html, relative_time};
use noye_shared::{Caller, Incident, SlaMultiReport, SlaReport, SlaSummary, Target};

/// Pretty-print a uptime ratio as a percentage with three decimal places, so
/// "five nines" deployments are visibly distinct from "four nines."
fn percent(ratio: f64) -> String {
    format!("{:.3}%", ratio * 100.0)
}

/// Format a duration in seconds as `Hh Mm Ss` or `Dd Hh Mm` depending on
/// magnitude. Keeps the table readable even for week-long windows.
fn format_duration(seconds: i64) -> String {
    if seconds <= 0 {
        return "0s".to_string();
    }
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, secs)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

/// Render the window-selector. Phase B: tabs (instead of a select+submit
/// form). Each tab is an `<a>` to the same page with the `?window=` param,
/// so the page captures the selection in the URL (shareable / bookmarkable)
/// without any JavaScript.
fn render_window_selector(current_window: &str) -> String {
    use crate::ui::layout::{Tab, tabs};
    let entries: Vec<(&str, &str, String)> = vec![
        ("24h", "Last 24 hours", "/stats?window=24h".to_string()),
        ("7d", "Last 7 days", "/stats?window=7d".to_string()),
        ("30d", "Last 30 days", "/stats?window=30d".to_string()),
        ("90d", "Last 90 days", "/stats?window=90d".to_string()),
    ];
    let active = entries
        .iter()
        .position(|(w, _, _)| *w == current_window)
        .unwrap_or(usize::MAX);
    // Borrow into Tab; the tabs helper only holds &str so the lifetime
    // of the underlying strings (the static labels and the just-built
    // hrefs) covers the call.
    let tab_items: Vec<Tab<'_>> = entries
        .iter()
        .map(|(_, label, href)| Tab { href, label })
        .collect();
    tabs(&tab_items, active, "Report window")
}

fn render_summary_card(summary: &SlaSummary) -> String {
    let target_count = summary.per_target.len();
    let total_downtime: i64 = summary.per_target.iter().map(|r| r.downtime_seconds).sum();
    let target_word = if target_count == 1 {
        "target"
    } else {
        "targets"
    };

    format!(
        r#"<section class="card" aria-labelledby="sla-summary-heading">
  <h3 id="sla-summary-heading">Overall</h3>
  <dl class="summary-grid">
    <div><dt>Window</dt><dd>{ws} → {we}</dd></div>
    <div><dt>Targets</dt><dd>{tc} {tw}</dd></div>
    <div><dt>Gross uptime</dt><dd>{gross}</dd></div>
    <div><dt>SLA uptime (excl. maintenance)</dt><dd>{sla}</dd></div>
    <div><dt>Total downtime</dt><dd>{dt}</dd></div>
  </dl>
</section>"#,
        ws = escape_html(&summary.window_start),
        we = escape_html(&summary.window_end),
        tc = target_count,
        tw = target_word,
        gross = percent(summary.overall_gross_uptime_ratio),
        sla = percent(summary.overall_sla_uptime_ratio),
        dt = format_duration(total_downtime),
    )
}

fn render_per_target_table(reports: &[SlaReport], current_window: &str) -> String {
    if reports.is_empty() {
        return r#"<section class="card"><p>No targets are visible to you.</p></section>"#
            .to_string();
    }

    let window_html = escape_html(current_window);
    let mut rows = String::new();
    for r in reports {
        let mttr_cell = match r.mttr_seconds {
            Some(s) => format_duration(s),
            None => "—".to_string(),
        };
        let maintenance_cell = if r.maintenance_seconds > 0 {
            format_duration(r.maintenance_seconds)
        } else {
            "—".to_string()
        };
        rows.push_str(&format!(
            r#"<tr>
  <td><a href="/stats/{tid}">{name}</a></td>
  <td><span class="badge" data-kind="uptime">{gross}</span></td>
  <td><span class="badge" data-kind="uptime">{sla}</span></td>
  <td>{dt}</td>
  <td>{maint}</td>
  <td>{ic}</td>
  <td>{mttr}</td>
  <td><a href="/api/stats/incidents/{tid}.csv?window={w}" class="btn btn-sm btn-ghost" aria-label="Download incidents for {name} as CSV">CSV</a></td>
</tr>"#,
            tid = escape_html(&r.target_id),
            name = escape_html(&r.target_name),
            gross = percent(r.gross_uptime_ratio),
            sla = percent(r.sla_uptime_ratio),
            dt = format_duration(r.downtime_seconds),
            maint = maintenance_cell,
            ic = r.incident_count,
            mttr = mttr_cell,
            w = window_html,
        ));
    }

    format!(
        r#"<section class="card" aria-labelledby="sla-targets-heading">
  <h3 id="sla-targets-heading">Per target</h3>
  <p style="margin-bottom:var(--space-sm);font-size:var(--fs-sm);color:var(--c-text-muted)">
    <strong>Gross uptime</strong> counts every minute of downtime. <strong>SLA uptime</strong> excludes downtime that fell entirely inside a scheduled maintenance window. Click a target name to drill down or "CSV" to export the incidents in this window.
  </p>
  <table aria-label="per-target SLA breakdown">
    <thead><tr>
      <th scope="col">Target</th>
      <th scope="col">Gross uptime</th>
      <th scope="col">SLA uptime</th>
      <th scope="col">Downtime</th>
      <th scope="col">Maintenance</th>
      <th scope="col">Incidents</th>
      <th scope="col">MTTR</th>
      <th scope="col"><span class="sr-only">Export</span></th>
    </tr></thead>
    <tbody>
      {rows}
    </tbody>
  </table>
</section>"#,
        rows = rows
    )
}

pub fn render_page(summary: &SlaSummary, current_window: &str, _caller: &Caller) -> String {
    let mut html = String::new();
    html.push_str(&render_window_selector(current_window));
    html.push_str(&render_summary_card(summary));
    html.push_str(&render_download_row(current_window));
    html.push_str(&render_per_target_table(
        &summary.per_target,
        current_window,
    ));
    html
}

/// Small download-CSV row above the per-target table. The link carries the
/// current window so the download reflects what's on screen.
fn render_download_row(current_window: &str) -> String {
    format!(
        r#"<div style="margin:var(--space-md) 0;display:flex;justify-content:flex-end">
  <a href="/api/stats/sla.csv?window={w}" class="button-link" aria-label="Download per-target SLA report as CSV for window {w}">
    Download CSV ({w})
  </a>
</div>"#,
        w = escape_html(current_window)
    )
}

// ─────────────────────────────────────────────
//  Per-target detail page (/stats/:id)
// ─────────────────────────────────────────────

/// Render the per-target SLA detail page.
///
/// `selected_window` controls which window the headline KPI card and the
/// incident list reflect; the multi-window comparison row is always 24h/7d/30d
/// regardless of selection.
pub fn render_detail(
    target: &Target,
    selected_window: &str,
    selected_report: &SlaReport,
    multi: &SlaMultiReport,
    incidents_in_window: &[Incident],
) -> String {
    let mut html = String::new();
    html.push_str(&render_detail_header(target, selected_report));
    html.push_str(&render_detail_window_selector(target, selected_window));
    html.push_str(&render_detail_kpi_card(selected_window, selected_report));
    html.push_str(&render_multi_window_comparison(multi));
    html.push_str(&render_detail_incident_list(
        target,
        selected_window,
        incidents_in_window,
    ));
    html
}

fn render_detail_header(target: &Target, selected_report: &SlaReport) -> String {
    // The card surfaces the things the operator wants to know in one glance:
    // who, where, how it's classified, and a quick link back to the
    // operations view. The selected-window uptime is repeated here so the
    // header itself is informative without scrolling.
    format!(
        r#"<section class="card" aria-labelledby="stats-detail-heading" style="margin-bottom:var(--space-md)">
  <h2 id="stats-detail-heading" style="margin-top:0">{name}</h2>
  <dl class="summary-grid">
    <div><dt>Type</dt><dd><span class="badge" data-kind="type">{ttype}</span></dd></div>
    <div><dt>Host</dt><dd><code>{host}</code></dd></div>
    <div><dt>Selected-window uptime</dt><dd><span class="badge" data-kind="uptime">{uptime}</span></dd></div>
    <div><dt>Operations</dt><dd><a href="/targets/{tid}">View target page</a></dd></div>
  </dl>
</section>"#,
        name = escape_html(&target.name),
        ttype = escape_html(&target.target_type),
        host = escape_html(&target.host),
        uptime = percent(selected_report.gross_uptime_ratio),
        tid = escape_html(&target.id),
    )
}

fn render_detail_window_selector(target: &Target, current_window: &str) -> String {
    use crate::ui::layout::{Tab, tabs};
    // Same control as on the index page, but routes to /stats/:id so the URL
    // captures both the target and the window. Bookmarkable, no JavaScript.
    let tid = escape_html(&target.id);
    let entries: Vec<(&str, &str, String)> = vec![
        ("24h", "Last 24 hours", format!("/stats/{tid}?window=24h")),
        ("7d", "Last 7 days", format!("/stats/{tid}?window=7d")),
        ("30d", "Last 30 days", format!("/stats/{tid}?window=30d")),
        ("90d", "Last 90 days", format!("/stats/{tid}?window=90d")),
    ];
    let active = entries
        .iter()
        .position(|(w, _, _)| *w == current_window)
        .unwrap_or(usize::MAX);
    let tab_items: Vec<Tab<'_>> = entries
        .iter()
        .map(|(_, label, href)| Tab { href, label })
        .collect();
    tabs(&tab_items, active, "Report window")
}

fn render_detail_kpi_card(window_label: &str, r: &SlaReport) -> String {
    let mttr = match r.mttr_seconds {
        Some(s) => format_duration(s),
        None => "—".to_string(),
    };
    let maintenance = if r.maintenance_seconds > 0 {
        format_duration(r.maintenance_seconds)
    } else {
        "—".to_string()
    };
    format!(
        r#"<section class="card" aria-labelledby="stats-detail-kpi">
  <h3 id="stats-detail-kpi">KPIs ({window})</h3>
  <dl class="summary-grid">
    <div><dt>Gross uptime</dt><dd>{gross}</dd></div>
    <div><dt>SLA uptime</dt><dd>{sla}</dd></div>
    <div><dt>Downtime</dt><dd>{dt}</dd></div>
    <div><dt>Maintenance</dt><dd>{maint}</dd></div>
    <div><dt>Incidents</dt><dd>{ic}</dd></div>
    <div><dt>MTTR</dt><dd>{mttr}</dd></div>
  </dl>
</section>"#,
        window = escape_html(window_label),
        gross = percent(r.gross_uptime_ratio),
        sla = percent(r.sla_uptime_ratio),
        dt = format_duration(r.downtime_seconds),
        maint = maintenance,
        ic = r.incident_count,
        mttr = mttr,
    )
}

/// Three columns side by side (24h / 7d / 30d) so divergence between
/// short-term and long-term reliability is visible at a glance.
fn render_multi_window_comparison(multi: &SlaMultiReport) -> String {
    let mut cols = String::new();
    for entry in &multi.windows {
        let r = &entry.report;
        let mttr = match r.mttr_seconds {
            Some(s) => format_duration(s),
            None => "—".to_string(),
        };
        cols.push_str(&format!(
            r#"<div class="card" style="margin:0">
  <h4 style="margin-top:0">{label}</h4>
  <dl class="summary-grid">
    <div><dt>Gross</dt><dd>{gross}</dd></div>
    <div><dt>SLA</dt><dd>{sla}</dd></div>
    <div><dt>Downtime</dt><dd>{dt}</dd></div>
    <div><dt>Incidents</dt><dd>{ic}</dd></div>
    <div><dt>MTTR</dt><dd>{mttr}</dd></div>
  </dl>
</div>"#,
            label = escape_html(&entry.label),
            gross = percent(r.gross_uptime_ratio),
            sla = percent(r.sla_uptime_ratio),
            dt = format_duration(r.downtime_seconds),
            ic = r.incident_count,
            mttr = mttr,
        ));
    }
    format!(
        r#"<section aria-labelledby="stats-multi-heading" style="margin:var(--space-md) 0">
  <h3 id="stats-multi-heading" style="margin-top:0">Multi-window comparison</h3>
  <div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(14rem,1fr));gap:var(--space-md)">
    {cols}
  </div>
  <p style="margin-top:var(--space-sm);font-size:0.875rem;color:var(--color-fg-muted)">
    These windows are independent of the selector above. Use them to spot whether short- and long-term reliability tell different stories.
  </p>
</section>"#,
        cols = cols
    )
}

fn render_detail_incident_list(target: &Target, window: &str, incidents: &[Incident]) -> String {
    let download_link = format!(
        r#"<a href="/api/stats/incidents/{tid}.csv?window={w}" class="button-link" aria-label="Download incidents as CSV for window {w}">Download CSV ({w})</a>"#,
        tid = escape_html(&target.id),
        w = escape_html(window),
    );

    if incidents.is_empty() {
        return format!(
            r#"<section class="card" aria-labelledby="stats-incidents-heading">
  <div style="display:flex;justify-content:space-between;align-items:center;gap:var(--space-md);flex-wrap:wrap">
    <h3 id="stats-incidents-heading" style="margin:0">Incidents in this window</h3>
    {download}
  </div>
  <p style="margin-top:var(--space-sm)">No incidents recorded for this target in the selected window. The CSV download contains the column header only.</p>
</section>"#,
            download = download_link
        );
    }

    let mut rows = String::new();
    for inc in incidents {
        let resolved = match inc.resolved_at.as_deref() {
            Some(ts) => relative_time(ts),
            None => r#"<em>still open</em>"#.to_string(),
        };
        let duration = match inc.duration_sec {
            Some(s) => format_duration(s),
            None => "—".to_string(),
        };
        let cause = inc.cause.as_deref().unwrap_or("—");
        rows.push_str(&format!(
            r#"<tr>
  <td>{opened}</td>
  <td>{resolved}</td>
  <td>{duration}</td>
  <td><span class="badge" data-kind="status">{status}</span></td>
  <td>{cause}</td>
</tr>"#,
            opened = relative_time(&inc.opened_at),
            resolved = resolved,
            duration = duration,
            status = escape_html(&inc.status),
            cause = escape_html(cause),
        ));
    }
    format!(
        r#"<section class="card" aria-labelledby="stats-incidents-heading">
  <div style="display:flex;justify-content:space-between;align-items:center;gap:var(--space-md);flex-wrap:wrap">
    <h3 id="stats-incidents-heading" style="margin:0">Incidents in this window</h3>
    {download}
  </div>
  <table aria-label="incidents during the selected window" style="margin-top:var(--space-sm)">
    <thead><tr>
      <th scope="col">Opened</th>
      <th scope="col">Resolved</th>
      <th scope="col">Duration</th>
      <th scope="col">Status</th>
      <th scope="col">Cause</th>
    </tr></thead>
    <tbody>
      {rows}
    </tbody>
  </table>
</section>"#,
        download = download_link,
        rows = rows
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_displays_three_decimal_places() {
        assert_eq!(percent(0.99999), "99.999%");
        assert_eq!(percent(1.0), "100.000%");
        assert_eq!(percent(0.0), "0.000%");
        assert_eq!(percent(0.95), "95.000%");
    }

    #[test]
    fn format_duration_picks_appropriate_units() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(125), "2m 5s");
        assert_eq!(format_duration(3700), "1h 1m 40s");
        assert_eq!(format_duration(90_000), "1d 1h 0m");
        assert_eq!(format_duration(7 * 86_400), "7d 0h 0m");
    }

    #[test]
    fn format_duration_handles_negative_input_defensively() {
        // Should never happen in practice, but the formatter shouldn't panic
        // or produce garbage if it does.
        assert_eq!(format_duration(-100), "0s");
    }

    #[test]
    fn window_selector_marks_current_window_active() {
        // Phase B: tabs replaced the <select>+submit form. The active
        // tab is identified by `aria-current="page"` on its <a>.
        let html = render_window_selector("7d");
        assert!(html.contains(r#"href="/stats?window=7d" aria-current="page""#));
        // Other windows are present as plain links without aria-current.
        assert!(html.contains(r#"href="/stats?window=24h""#));
        // Only one current marker.
        assert_eq!(html.matches(r#"aria-current="page""#).count(), 1);
    }

    #[test]
    fn window_selector_handles_unknown_value_gracefully() {
        // No tab receives aria-current; the page renders all four
        // options as plain links.
        let html = render_window_selector("foo");
        assert!(!html.contains(r#"aria-current="page""#));
        assert!(html.contains(r#"href="/stats?window=24h""#));
    }

    #[test]
    fn window_selector_emits_no_form_or_button() {
        // Sanity guard: the Phase B tabs must not regress to the old
        // <form><select><button> shape.
        let html = render_window_selector("24h");
        assert!(!html.contains("<form"));
        assert!(!html.contains("<select"));
    }

    #[test]
    fn per_target_table_includes_per_row_csv_link() {
        use noye_shared::SlaReport;
        let reports = vec![SlaReport {
            target_id: "t-abc".to_string(),
            target_name: "web-01".to_string(),
            window_start: "2026-01-01T00:00:00Z".to_string(),
            window_end: "2026-01-02T00:00:00Z".to_string(),
            window_seconds: 86_400,
            gross_uptime_ratio: 0.999,
            sla_uptime_ratio: 0.999,
            downtime_seconds: 86,
            maintenance_seconds: 0,
            incident_count: 1,
            mttr_seconds: Some(86),
        }];
        let html = render_per_target_table(&reports, "24h");
        // Each row has an Incidents-CSV link with the current window.
        assert!(html.contains(r#"href="/api/stats/incidents/t-abc.csv?window=24h""#));
    }

    #[test]
    fn per_target_table_includes_interpretation_help() {
        use noye_shared::SlaReport;
        let reports = vec![SlaReport {
            target_id: "t".to_string(),
            target_name: "n".to_string(),
            window_start: "".into(),
            window_end: "".into(),
            window_seconds: 0,
            gross_uptime_ratio: 1.0,
            sla_uptime_ratio: 1.0,
            downtime_seconds: 0,
            maintenance_seconds: 0,
            incident_count: 0,
            mttr_seconds: None,
        }];
        let html = render_per_target_table(&reports, "24h");
        // The "Gross uptime counts every minute" / "SLA uptime excludes
        // … maintenance window" interpretation must accompany the table.
        let lower = html.to_lowercase();
        assert!(lower.contains("gross uptime"));
        assert!(lower.contains("sla uptime"));
        assert!(lower.contains("maintenance"));
    }
}
