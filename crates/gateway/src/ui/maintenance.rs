//! Maintenance windows page.
//!
//! ## Phase C re-design
//!
//! The page is renamed conceptually from "Maintenance" (which sounds
//! like the system stops watching) to "**Notification suppression**":
//! during a window, the monitor *still runs*, incidents *still record*,
//! but state-change *notifications are not dispatched*. The SLA
//! calculation excludes the window's seconds from its denominator so
//! the operator's planned downtime doesn't depress reliability metrics.
//!
//! What this page now communicates clearly:
//!
//! 1. **A help card at the top** explaining what a window does (and
//!    doesn't do): monitor keeps running → incidents keep recording →
//!    notifications are suppressed → SLA denominator excludes the time.
//! 2. **Active vs upcoming/past sections** so an operator scanning the
//!    page can immediately tell which windows are currently affecting
//!    behaviour.
//! 3. **UTC timestamps** displayed via `<time datetime>` so the browser
//!    can show a localised tooltip while the canonical UTC value
//!    remains in the DOM (and in screen-reader output).
//! 4. **"Suppress notify"** column renamed to a clearer "Notifications"
//!    column with explicit "suppressed" / "active" labels.

use noye_shared::{Caller, MaintenanceWindow};

use crate::ui::layout::{card, escape_html, status_badge, time_local};

// ──────────────────────────────────────────────────────────────────
//  Pure-logic helpers
// ──────────────────────────────────────────────────────────────────

/// Partition windows into `(active, upcoming_or_past)` preserving order.
///
/// `is_active` on `MaintenanceWindow` is the source of truth (computed
/// at fetch time on Core based on the current time vs. start/end).
/// We trust it here rather than re-deriving from `start_at`/`end_at`,
/// which would require parsing timestamps and currying current-time in.
pub fn partition_windows<'a>(
    windows: &'a [MaintenanceWindow],
) -> (Vec<&'a MaintenanceWindow>, Vec<&'a MaintenanceWindow>) {
    let mut active = Vec::new();
    let mut other = Vec::new();
    for w in windows {
        if w.is_active {
            active.push(w);
        } else {
            other.push(w);
        }
    }
    (active, other)
}

/// Render the human-readable scope string for a window.
///
/// Scope precedence: a specific target (target_id) wins over a tag
/// (target_tag); without either it's "All targets". This matches the
/// Core's apply-window logic.
pub fn format_scope(window: &MaintenanceWindow) -> String {
    if let Some(ref tid) = window.target_id {
        format!("Target: {}", escape_html(tid))
    } else if let Some(ref tag) = window.target_tag {
        format!("Tag: {}", escape_html(tag))
    } else {
        "All targets".to_string()
    }
}

// ──────────────────────────────────────────────────────────────────
//  Page rendering
// ──────────────────────────────────────────────────────────────────

pub fn render_list(windows: &[MaintenanceWindow], caller: &Caller) -> String {
    let mut html = String::new();
    html.push_str(&render_help_card());

    if caller.is_admin() {
        let body = r#"<details>
  <summary><strong>+ Schedule a notification suppression window</strong></summary>
  <div style="margin-top:var(--space-md)">
    <p>Schedule via the API:</p>
    <p><code>POST /api/maintenance</code> with a JSON body including <code>name</code>, <code>start_at</code> (UTC, ISO 8601), <code>end_at</code> (UTC, ISO 8601), and either <code>target_id</code> or <code>target_tag</code> (or neither, to apply to every target).</p>
  </div>
</details>"#;
        html.push_str(&card("Manage windows", "maint-manage", body));
    }

    let (active, other) = partition_windows(windows);
    html.push_str(&render_section("Active windows", "maint-active", &active, true));
    html.push_str(&render_section(
        "Upcoming and past windows",
        "maint-other",
        &other,
        false,
    ));
    html
}

fn render_help_card() -> String {
    let body = r#"<p>A <strong>notification suppression window</strong> tells Noye to keep watching but to stay quiet during the configured time range. Specifically:</p>
<ul style="margin-top:var(--space-sm);padding-left:var(--space-lg)">
  <li>The monitor continues running every minute as scheduled.</li>
  <li>Incidents are <em>still recorded</em> in the audit-visible list — operators can review what happened during the window after the fact.</li>
  <li>Outbound notifications (webhook / Slack / email) are <em>not</em> dispatched for state changes that fall inside the window.</li>
  <li>Time inside the window is excluded from the SLA-uptime denominator, so planned downtime does not depress your reliability metrics.</li>
</ul>"#;
    card("How a window works", "maint-help", body)
}

fn render_section(
    title: &str,
    id: &str,
    windows: &[&MaintenanceWindow],
    badge_active: bool,
) -> String {
    if windows.is_empty() {
        let body = if badge_active {
            r#"<p role="status">No suppression windows are active right now.</p>"#
        } else {
            r#"<p role="status">No upcoming or past windows recorded.</p>"#
        };
        return card(title, id, body);
    }

    let mut body = String::new();
    body.push_str(r#"<table>"#);
    body.push_str(r#"<thead><tr>"#);
    body.push_str(r#"<th scope="col">Status</th>"#);
    body.push_str(r#"<th scope="col">Name</th>"#);
    body.push_str(r#"<th scope="col">Scope</th>"#);
    body.push_str(r#"<th scope="col">Start (UTC)</th>"#);
    body.push_str(r#"<th scope="col">End (UTC)</th>"#);
    body.push_str(r#"<th scope="col">Notifications</th>"#);
    body.push_str(r#"<th scope="col">Created by</th>"#);
    body.push_str("</tr></thead><tbody>");

    for w in windows {
        let status_label = if badge_active {
            // The window is in its active interval — flag it as
            // "maintenance" (the badge maps to the maint tone).
            status_badge("maintenance")
        } else {
            // Past or future — render a quiet "scheduled" badge.
            r#"<span class="badge badge-info">scheduled</span>"#.to_string()
        };
        let notify_label = if w.suppress_notify {
            // The window is what makes notifications quiet; default-on.
            r#"<span class="badge badge-maint" aria-label="Notifications suppressed">suppressed</span>"#
        } else {
            // Edge case: a window with suppress_notify=false has no
            // notification effect — operators sometimes use these to
            // mark a planned event without changing pager behaviour.
            r#"<span class="badge badge-info" aria-label="Notifications still firing">unaffected</span>"#
        };
        body.push_str("<tr>");
        body.push_str(&format!("<td>{}</td>", status_label));
        body.push_str(&format!("<td>{}</td>", escape_html(&w.name)));
        body.push_str(&format!("<td>{}</td>", format_scope(w)));
        body.push_str(&format!("<td>{}</td>", time_local(&w.start_at)));
        body.push_str(&format!("<td>{}</td>", time_local(&w.end_at)));
        body.push_str(&format!("<td>{}</td>", notify_label));
        body.push_str(&format!("<td>{}</td>", escape_html(&w.created_by)));
        body.push_str("</tr>");
    }
    body.push_str("</tbody></table>");
    card(title, id, &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_window(id: &str, name: &str, is_active: bool) -> MaintenanceWindow {
        MaintenanceWindow {
            id: id.into(),
            name: name.into(),
            start_at: "2026-04-29T10:00:00Z".into(),
            end_at: "2026-04-29T12:00:00Z".into(),
            target_tag: None,
            target_id: None,
            suppress_notify: true,
            is_active,
            created_at: "2026-04-28T09:00:00Z".into(),
            created_by: "alice@example.com".into(),
            updated_by: "alice@example.com".into(),
        }
    }

    fn fake_caller(role: &str) -> Caller {
        Caller {
            user_id: "u1".into(),
            email: "u@x".into(),
            name: "U".into(),
            role: role.into(),
        }
    }

    // ── partition_windows ──

    #[test]
    fn partition_separates_active_from_other() {
        let list = vec![
            fake_window("1", "morning patch", true),
            fake_window("2", "evening patch", false),
            fake_window("3", "another active", true),
        ];
        let (active, other) = partition_windows(&list);
        assert_eq!(active.len(), 2);
        assert_eq!(other.len(), 1);
        assert_eq!(active[0].id, "1");
        assert_eq!(active[1].id, "3");
        assert_eq!(other[0].id, "2");
    }

    #[test]
    fn partition_preserves_caller_order() {
        let list = vec![
            fake_window("z", "z", false),
            fake_window("a", "a", false),
        ];
        let (_, other) = partition_windows(&list);
        let ids: Vec<&str> = other.iter().map(|w| w.id.as_str()).collect();
        assert_eq!(ids, vec!["z", "a"]);
    }

    // ── format_scope ──

    #[test]
    fn scope_prefers_target_id_over_tag() {
        let mut w = fake_window("1", "x", true);
        w.target_id = Some("t-abc".into());
        w.target_tag = Some("prod".into());
        // Specific target wins over the tag — the apply-logic on Core
        // is the same way around.
        assert_eq!(format_scope(&w), "Target: t-abc");
    }

    #[test]
    fn scope_falls_back_to_tag_when_no_target() {
        let mut w = fake_window("1", "x", true);
        w.target_tag = Some("prod".into());
        assert_eq!(format_scope(&w), "Tag: prod");
    }

    #[test]
    fn scope_renders_all_targets_when_neither_set() {
        let w = fake_window("1", "x", true);
        assert_eq!(format_scope(&w), "All targets");
    }

    #[test]
    fn scope_escapes_html_in_inputs() {
        // The fields come from D1; we don't expect HTML in them, but
        // the renderer escapes anyway as a defense-in-depth.
        let mut w = fake_window("1", "x", true);
        w.target_id = Some("<bad>".into());
        let s = format_scope(&w);
        assert!(s.contains("&lt;bad&gt;"));
        assert!(!s.contains("<bad>"));
    }

    // ── render_list ──

    #[test]
    fn list_includes_help_card_explaining_terminology() {
        let html = render_list(&[], &fake_caller("admin"));
        // The help card covers the four bullets that distinguish a
        // suppression window from a "maintenance shutdown."
        let lower = html.to_lowercase();
        assert!(lower.contains("notification suppression"));
        assert!(lower.contains("monitor"));
        assert!(lower.contains("incident"));
        assert!(lower.contains("sla"));
    }

    #[test]
    fn list_admin_sees_management_block_member_does_not() {
        let admin_html = render_list(&[], &fake_caller("admin"));
        let member_html = render_list(&[], &fake_caller("member"));
        assert!(admin_html.contains("Manage windows"));
        assert!(!member_html.contains("Manage windows"));
    }

    #[test]
    fn list_renders_active_section_with_maintenance_badge_for_active_windows() {
        let list = vec![fake_window("1", "morning patch", true)];
        let html = render_list(&list, &fake_caller("admin"));
        assert!(html.contains(r#"id="maint-active""#));
        // Active windows get the "maintenance" status badge (which the
        // BadgeKind enum maps to badge-maint).
        assert!(html.contains("badge-maint"));
    }

    #[test]
    fn list_renders_other_section_with_scheduled_badge_for_inactive_windows() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"));
        assert!(html.contains(r#"id="maint-other""#));
        assert!(html.contains(r#"badge badge-info"#));
        assert!(html.contains(">scheduled<"));
    }

    #[test]
    fn list_friendly_message_when_no_active_windows() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"));
        // The active-section card still renders, but its body says "no
        // active windows" rather than an empty table.
        assert!(html.contains("No suppression windows are active"));
    }

    #[test]
    fn list_uses_time_element_for_utc_timestamps() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"));
        // <time datetime="..."> is rendered for both start and end.
        assert!(html.contains(r#"<time datetime="2026-04-29T10:00:00Z""#));
        assert!(html.contains(r#"<time datetime="2026-04-29T12:00:00Z""#));
        // Column headers explicitly mark the timezone.
        assert!(html.contains("Start (UTC)"));
        assert!(html.contains("End (UTC)"));
    }

    #[test]
    fn list_renders_notification_suppression_label() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"));
        // The "Notifications" column shows "suppressed" — explicit,
        // not the boolean string "true".
        assert!(html.contains(">suppressed<"));
        assert!(!html.contains(">true<"));
    }

    #[test]
    fn list_renders_unaffected_label_when_suppress_notify_false() {
        let mut w = fake_window("1", "marker only", false);
        w.suppress_notify = false;
        let html = render_list(&[w], &fake_caller("admin"));
        assert!(html.contains(">unaffected<"));
    }

    #[test]
    fn list_escapes_caller_name() {
        let mut w = fake_window("1", "tonight", false);
        w.created_by = "<script>".into();
        let html = render_list(&[w], &fake_caller("admin"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
