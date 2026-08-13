//! Dashboard — "today's must-see," not a directory listing.
//!
//! ## Phase B re-design
//!
//! The Phase A token system gave us `metric_card` and `card`; this file
//! uses them to surface the four numbers an operator most needs at a
//! glance, then lists what's actively wrong.
//!
//! Layout:
//!
//! 1. **Metric strip** — Targets, Up, Down, Open incidents. Tone is
//!    derived from the values themselves (Down > 0 → red,
//!    Open incidents > 0 → degraded).
//! 2. **Open-incidents card** — current open incidents only, sorted as
//!    received (caller passes them in opened_at DESC). Empty state is
//!    a friendly "all clear" message.
//! 3. **Status breakdown card** — degraded/maintenance/unknown/disabled
//!    counts. These are usually 0 and don't deserve top-strip space, but
//!    are useful when non-zero, so they live in a secondary block.
//!
//! ## What's intentionally not on the dashboard
//!
//! - "Next checks coming up" — the data model doesn't carry a
//!   `next_check_at` column today. Adding it would require a `target_states`
//!   schema bump; deferred until there's actual demand.
//! - Resolved incidents — the dashboard's job is "what needs attention,"
//!   not historical view. Resolved incidents live on `/incidents` (under
//!   a `<details>` collapsed by default).
//! - Auto-refresh toggle — deferred. The page is server-rendered and
//!   reloading is one keystroke; an auto-refresher would mostly create
//!   demand for `prefers-reduced-motion`-aware polling, which is more
//!   work than it's worth before we know operators want it.

use noye_shared::{Incident, StatusSummary};

use crate::ui::layout::{MetricTone, card, escape_html, metric_card, status_badge, time_local};

// ──────────────────────────────────────────────────────────────────
//  Pure-logic helpers
// ──────────────────────────────────────────────────────────────────

/// Derive the metric tone (color) from the value being displayed.
///
/// `kind == "down"` is red whenever count > 0; `"open"` (incidents) is
/// degraded whenever count > 0; everything else is the default tone.
/// Pure helper so the colour-mapping policy is unit-testable and
/// matches the page rendering 1:1.
pub fn metric_tone_for(kind: &str, value: i64) -> MetricTone {
    match (kind, value) {
        ("down", n) if n > 0 => MetricTone::Down,
        ("open", n) if n > 0 => MetricTone::Degraded,
        ("up", n) if n > 0 => MetricTone::Up,
        _ => MetricTone::Default,
    }
}

/// Build the small "X up, Y down" hint shown under the Targets metric.
pub fn targets_hint(summary: &StatusSummary) -> String {
    format!("{} up · {} down", summary.up, summary.down)
}

/// Filter the incidents list to those still open. Pure helper used by
/// the metric strip ("Open incidents" count) and by the body ("Open
/// incidents" card).
///
/// Order is preserved — caller controls display order (typically
/// opened_at DESC).
pub fn select_open(incidents: &[Incident]) -> Vec<&Incident> {
    incidents.iter().filter(|i| i.status == "open").collect()
}

// ──────────────────────────────────────────────────────────────────
//  Page rendering
// ──────────────────────────────────────────────────────────────────

pub fn render(summary: &StatusSummary, recent_incidents: &[Incident]) -> String {
    let open = select_open(recent_incidents);
    let mut html = String::new();
    html.push_str(&render_metric_strip(summary, open.len() as i64));
    html.push_str(&render_open_incidents_card(&open));
    html.push_str(&render_breakdown_card(summary));
    html
}

fn render_metric_strip(summary: &StatusSummary, open_count: i64) -> String {
    let cards: String = [
        metric_card(
            "Targets",
            &summary.total.to_string(),
            Some(&targets_hint(summary)),
            MetricTone::Default,
        ),
        metric_card(
            "Up",
            &summary.up.to_string(),
            None,
            metric_tone_for("up", summary.up),
        ),
        metric_card(
            "Down",
            &summary.down.to_string(),
            None,
            metric_tone_for("down", summary.down),
        ),
        metric_card(
            "Open incidents",
            &open_count.to_string(),
            None,
            metric_tone_for("open", open_count),
        ),
    ]
    .concat();

    format!(
        r#"<section aria-label="System overview"><div class="metric-grid">{cards}</div></section>"#
    )
}

fn render_open_incidents_card(open: &[&Incident]) -> String {
    let body = if open.is_empty() {
        r#"<p role="status">All clear — no open incidents right now.</p>"#.to_string()
    } else {
        let mut s = String::new();
        s.push_str(r#"<table>"#);
        s.push_str(r#"<thead><tr><th scope="col">Status</th><th scope="col">Target</th><th scope="col">Cause</th><th scope="col">Opened</th></tr></thead>"#);
        s.push_str("<tbody>");
        for inc in open {
            s.push_str("<tr>");
            s.push_str(&format!("<td>{}</td>", status_badge(&inc.status)));
            s.push_str(&format!(
                r#"<td><a href="/targets/{id}">{id}</a></td>"#,
                id = escape_html(&inc.target_id),
            ));
            s.push_str(&format!(
                "<td>{}</td>",
                escape_html(inc.cause.as_deref().unwrap_or("—"))
            ));
            s.push_str(&format!("<td>{}</td>", time_local(&inc.opened_at)));
            s.push_str("</tr>");
        }
        s.push_str("</tbody></table>");
        s.push_str(
            r#"<p style="margin-top:var(--space-md)"><a href="/incidents">View all incidents →</a></p>"#,
        );
        s
    };
    card("Open incidents", "dashboard-open", &body)
}

fn render_breakdown_card(summary: &StatusSummary) -> String {
    // Only render the breakdown if at least one non-up/down value is
    // non-zero. Hiding all-zeros keeps the dashboard quiet on a healthy
    // system.
    let interesting = summary.degraded + summary.maintenance + summary.unknown + summary.disabled;
    if interesting == 0 {
        return String::new();
    }
    let body = format!(
        r#"<dl class="info-grid">
  <dt>Degraded</dt><dd>{degraded}</dd>
  <dt>Maintenance</dt><dd>{maintenance}</dd>
  <dt>Unknown</dt><dd>{unknown}</dd>
  <dt>Disabled</dt><dd>{disabled}</dd>
</dl>"#,
        degraded = summary.degraded,
        maintenance = summary.maintenance,
        unknown = summary.unknown,
        disabled = summary.disabled,
    );
    card("Status breakdown", "dashboard-breakdown", &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_summary() -> StatusSummary {
        StatusSummary {
            total: 0,
            up: 0,
            down: 0,
            degraded: 0,
            maintenance: 0,
            unknown: 0,
            disabled: 0,
        }
    }

    fn fake_incident(id: &str, target: &str, status: &str) -> Incident {
        Incident {
            id: id.into(),
            target_id: target.into(),
            status: status.into(),
            opened_at: "2026-04-29T10:00:00Z".into(),
            resolved_at: None,
            duration_sec: None,
            cause: Some("HTTP 503".into()),
            resolution_note: None,
            opened_by: Some("system".into()),
            resolved_by: None,
        }
    }

    // ── metric_tone_for ──

    #[test]
    fn metric_tone_down_is_red_only_when_nonzero() {
        assert_eq!(metric_tone_for("down", 0), MetricTone::Default);
        assert_eq!(metric_tone_for("down", 1), MetricTone::Down);
        assert_eq!(metric_tone_for("down", 50), MetricTone::Down);
    }

    #[test]
    fn metric_tone_open_is_degraded_only_when_nonzero() {
        assert_eq!(metric_tone_for("open", 0), MetricTone::Default);
        assert_eq!(metric_tone_for("open", 1), MetricTone::Degraded);
    }

    #[test]
    fn metric_tone_up_is_green_only_when_nonzero() {
        // "Up = 0" is unusual (it means the system has zero healthy
        // targets) and shouldn't be highlighted in green; the default
        // tone is correct.
        assert_eq!(metric_tone_for("up", 0), MetricTone::Default);
        assert_eq!(metric_tone_for("up", 5), MetricTone::Up);
    }

    #[test]
    fn metric_tone_unknown_kind_is_default() {
        assert_eq!(metric_tone_for("garbage", 99), MetricTone::Default);
    }

    // ── targets_hint ──

    #[test]
    fn targets_hint_format() {
        let mut s = empty_summary();
        s.up = 12;
        s.down = 3;
        assert_eq!(targets_hint(&s), "12 up · 3 down");
    }

    // ── select_open ──

    #[test]
    fn select_open_returns_only_open() {
        let list = vec![
            fake_incident("1", "t", "open"),
            fake_incident("2", "t", "resolved"),
            fake_incident("3", "t", "open"),
        ];
        let open = select_open(&list);
        assert_eq!(open.len(), 2);
        assert_eq!(open[0].id, "1");
        assert_eq!(open[1].id, "3");
    }

    #[test]
    fn select_open_preserves_caller_order() {
        let list = vec![
            fake_incident("z", "t", "open"),
            fake_incident("a", "t", "open"),
        ];
        let open = select_open(&list);
        assert_eq!(open[0].id, "z");
        assert_eq!(open[1].id, "a");
    }

    #[test]
    fn select_open_handles_empty() {
        assert!(select_open(&[]).is_empty());
    }

    // ── render ──

    #[test]
    fn render_metric_strip_shows_four_cards() {
        let mut s = empty_summary();
        s.total = 10;
        s.up = 7;
        s.down = 3;
        let html = render(&s, &[]);
        // Four labels appear in the metric strip in order.
        let labels = ["Targets", "Up", "Down", "Open incidents"];
        let mut seen = 0;
        let mut cursor = 0;
        for label in labels {
            let needle = format!(">{label}<");
            match html[cursor..].find(&needle) {
                Some(idx) => {
                    cursor += idx + needle.len();
                    seen += 1;
                }
                None => panic!("metric label '{label}' not found in dashboard"),
            }
        }
        assert_eq!(seen, 4);
    }

    #[test]
    fn render_targets_metric_includes_hint() {
        let mut s = empty_summary();
        s.total = 10;
        s.up = 7;
        s.down = 3;
        let html = render(&s, &[]);
        assert!(html.contains("7 up · 3 down"));
    }

    #[test]
    fn render_open_incidents_card_is_friendly_when_empty() {
        let html = render(&empty_summary(), &[]);
        assert!(html.contains("All clear"));
        assert!(!html.contains("<table"));
    }

    #[test]
    fn render_open_incidents_card_lists_open_only() {
        let list = vec![
            fake_incident("1", "web-01", "open"),
            fake_incident("2", "db-01", "resolved"),
        ];
        let html = render(&empty_summary(), &list);
        // The open card includes the open incident…
        assert!(html.contains(r#"href="/targets/web-01""#));
        // …but the resolved one is filtered out.
        assert!(!html.contains(r#"href="/targets/db-01""#));
    }

    #[test]
    fn render_breakdown_card_omitted_when_all_zero() {
        let html = render(&empty_summary(), &[]);
        assert!(!html.contains("Status breakdown"));
    }

    #[test]
    fn render_breakdown_card_appears_when_any_nonzero() {
        let mut s = empty_summary();
        s.maintenance = 1;
        let html = render(&s, &[]);
        assert!(html.contains("Status breakdown"));
        assert!(html.contains("Maintenance"));
    }

    #[test]
    fn render_open_count_in_metric_strip_matches_open_incidents() {
        // Two incidents, only one open — the "Open incidents" metric
        // should show 1, not 2.
        let list = vec![
            fake_incident("1", "t", "open"),
            fake_incident("2", "t", "resolved"),
        ];
        let html = render(&empty_summary(), &list);
        // We don't pin the exact rendered HTML for the metric value,
        // but we check the value "1" appears somewhere after the
        // "Open incidents" label and before the next card.
        let label_pos = html.find(">Open incidents<").expect("label missing");
        let after_label = &html[label_pos..];
        assert!(after_label.contains(">1<"));
    }
}
