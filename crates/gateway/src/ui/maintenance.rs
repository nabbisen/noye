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
pub fn partition_windows(
    windows: &[MaintenanceWindow],
) -> (Vec<&MaintenanceWindow>, Vec<&MaintenanceWindow>) {
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

pub fn render_list(windows: &[MaintenanceWindow], caller: &Caller, csrf: Option<&str>) -> String {
    let mut html = String::new();
    html.push_str(&render_help_card());

    if caller.is_admin() {
        html.push_str(&card(
            "Schedule a suppression window",
            "maint-manage",
            &render_create_form(csrf),
        ));
    }

    let (active, other) = partition_windows(windows);
    html.push_str(&render_section(
        "Active windows",
        "maint-active",
        &active,
        true,
    ));
    html.push_str(&render_section(
        "Upcoming and past windows",
        "maint-other",
        &other,
        false,
    ));
    html
}

/// The create form (subject 11, G-07). Three points the handoff insists
/// on, all structural rather than scripted:
///
/// 1. **Three named situations as radios**, not a single checkbox — each
///    radio states its own consequence for both `suppress_notify` and
///    `exclude_from_sla` (DEC-013), so choosing one *is* choosing both
///    flags at once, correctly, every time.
/// 2. **Scope as radios**, not two free-standing text fields — a target
///    and a tag typed into two boxes at once would hit the new
///    `CHECK (NOT (target_id IS NOT NULL AND target_tag IS NOT NULL))`
///    constraint (subject 12) and bounce with a database error the
///    operator can't act on. Radios make the ambiguous combination
///    impossible to submit in the first place, not merely rejected.
/// 3. **A real `<form method="post" action="/maintenance">`**, not a
///    `fetch()`-driven one — this screen must work with scripting
///    disabled (NFR-A11Y-10). See `verify_csrf_form` in `gateway::lib`
///    for how the CSRF token travels as a hidden field instead of the
///    `X-CSRF-Token` header the rest of this app's forms use.
fn render_create_form(csrf: Option<&str>) -> String {
    let csrf_field = match csrf {
        Some(t) => format!(
            r#"<input type="hidden" name="csrf_token" value="{}">"#,
            escape_html(t)
        ),
        None => String::new(),
    };
    format!(
        r#"<form method="post" action="/maintenance" style="display:grid;gap:var(--space-md);max-width:40rem">
  {csrf_field}
  <label>Name <input type="text" name="name" required></label>
  <label>Start (UTC, ISO 8601) <input type="text" name="start_at" placeholder="2026-08-13T14:00:00Z" required></label>
  <label>End (UTC, ISO 8601) <input type="text" name="end_at" placeholder="2026-08-13T16:00:00Z" required></label>

  <fieldset>
    <legend>Situation</legend>
    <label><input type="radio" name="situation" value="planned" checked>
      Planned maintenance — notifications suppressed, SLA excluded (default)</label>
    <label><input type="radio" name="situation" value="outage">
      Known external outage — notifications suppressed, SLA <strong>still counts</strong> the downtime</label>
    <label><input type="radio" name="situation" value="noise">
      Expected noise — notifications <strong>still fire</strong>, SLA excluded</label>
  </fieldset>

  <fieldset>
    <legend>Scope</legend>
    <label><input type="radio" name="scope_kind" value="all" checked> All targets</label>
    <label><input type="radio" name="scope_kind" value="target"> Specific target ID
      <input type="text" name="target_id" placeholder="target id"></label>
    <label><input type="radio" name="scope_kind" value="tag"> Tag
      <input type="text" name="target_tag" placeholder="tag"></label>
  </fieldset>

  <button type="submit">Schedule window</button>
</form>"#
    )
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
    body.push_str(r#"<th scope="col">SLA</th>"#);
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
        // T-57 (subject 11, NFR-A11Y-03): both behaviours as text, not
        // colour alone -- "excluded"/"counted" reads correctly even
        // without seeing the badge's tone.
        let sla_label = if w.exclude_from_sla {
            r#"<span class="badge badge-maint" aria-label="Excluded from SLA">excluded</span>"#
        } else {
            r#"<span class="badge badge-info" aria-label="Counted toward SLA">counted</span>"#
        };
        body.push_str("<tr>");
        body.push_str(&format!("<td>{}</td>", status_label));
        body.push_str(&format!("<td>{}</td>", escape_html(&w.name)));
        body.push_str(&format!("<td>{}</td>", format_scope(w)));
        body.push_str(&format!("<td>{}</td>", time_local(&w.start_at)));
        body.push_str(&format!("<td>{}</td>", time_local(&w.end_at)));
        body.push_str(&format!("<td>{}</td>", notify_label));
        body.push_str(&format!("<td>{}</td>", sla_label));
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
            exclude_from_sla: true,
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
        let list = vec![fake_window("z", "z", false), fake_window("a", "a", false)];
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
        let html = render_list(&[], &fake_caller("admin"), None);
        // The help card covers the four bullets that distinguish a
        // suppression window from a "maintenance shutdown."
        let lower = html.to_lowercase();
        assert!(lower.contains("notification suppression"));
        assert!(lower.contains("monitor"));
        assert!(lower.contains("incident"));
        assert!(lower.contains("sla"));
    }

    #[test]
    fn list_admin_sees_create_form_member_does_not() {
        let admin_html = render_list(&[], &fake_caller("admin"), None);
        let member_html = render_list(&[], &fake_caller("member"), None);
        assert!(admin_html.contains(r#"<form method="post" action="/maintenance""#));
        assert!(!member_html.contains(r#"<form method="post" action="/maintenance""#));
    }

    #[test]
    fn list_renders_active_section_with_maintenance_badge_for_active_windows() {
        let list = vec![fake_window("1", "morning patch", true)];
        let html = render_list(&list, &fake_caller("admin"), None);
        assert!(html.contains(r#"id="maint-active""#));
        // Active windows get the "maintenance" status badge (which the
        // BadgeKind enum maps to badge-maint).
        assert!(html.contains("badge-maint"));
    }

    #[test]
    fn list_renders_other_section_with_scheduled_badge_for_inactive_windows() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"), None);
        assert!(html.contains(r#"id="maint-other""#));
        assert!(html.contains(r#"badge badge-info"#));
        assert!(html.contains(">scheduled<"));
    }

    #[test]
    fn list_friendly_message_when_no_active_windows() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"), None);
        // The active-section card still renders, but its body says "no
        // active windows" rather than an empty table.
        assert!(html.contains("No suppression windows are active"));
    }

    #[test]
    fn list_uses_time_element_for_utc_timestamps() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"), None);
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
        let html = render_list(&list, &fake_caller("admin"), None);
        // The "Notifications" column shows "suppressed" — explicit,
        // not the boolean string "true".
        assert!(html.contains(">suppressed<"));
        assert!(!html.contains(">true<"));
    }

    #[test]
    fn list_renders_unaffected_label_when_suppress_notify_false() {
        let mut w = fake_window("1", "marker only", false);
        w.suppress_notify = false;
        let html = render_list(&[w], &fake_caller("admin"), None);
        assert!(html.contains(">unaffected<"));
    }

    #[test]
    fn list_escapes_caller_name() {
        let mut w = fake_window("1", "tonight", false);
        w.created_by = "<script>".into();
        let html = render_list(&[w], &fake_caller("admin"), None);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }

    // ── T-57: SLA column states the exclude_from_sla behaviour in text ──

    #[test]
    fn list_renders_excluded_label_when_exclude_from_sla_true() {
        let list = vec![fake_window("1", "tonight", false)];
        let html = render_list(&list, &fake_caller("admin"), None);
        assert!(html.contains(">excluded<"));
    }

    #[test]
    fn list_renders_counted_label_when_exclude_from_sla_false() {
        let mut w = fake_window("1", "known outage", false);
        w.exclude_from_sla = false;
        let html = render_list(&[w], &fake_caller("admin"), None);
        assert!(html.contains(">counted<"));
    }

    #[test]
    fn list_shows_both_behaviours_independently() {
        // T-55's shape from the listing side: a window that silences
        // notifications but does NOT exclude SLA time -- the "Known
        // external outage" situation -- must show "suppressed" AND
        // "counted" together, not one flag implying the other.
        let mut w = fake_window("1", "known outage", true);
        w.exclude_from_sla = false;
        let html = render_list(&[w], &fake_caller("admin"), None);
        assert!(html.contains(">suppressed<"));
        assert!(html.contains(">counted<"));
    }

    // ── render_create_form (subject 11, NFR-A11Y-10) ──

    #[test]
    fn create_form_is_a_real_no_js_post_form() {
        let html = render_list(&[], &fake_caller("admin"), None);
        assert!(html.contains(r#"<form method="post" action="/maintenance""#));
        assert!(!html.contains("fetch("));
    }

    #[test]
    fn create_form_offers_three_named_situations_as_radios() {
        let html = render_list(&[], &fake_caller("admin"), None);
        assert!(html.contains(r#"name="situation" value="planned""#));
        assert!(html.contains(r#"name="situation" value="outage""#));
        assert!(html.contains(r#"name="situation" value="noise""#));
        assert!(html.contains("Planned maintenance"));
        assert!(html.contains("Known external outage"));
        assert!(html.contains("Expected noise"));
    }

    #[test]
    fn create_form_scope_is_radios_not_two_free_fields() {
        // Scope must be a choice (radios), not two independently-fillable
        // text boxes -- filling both target_id and target_tag would trip
        // the CHECK constraint subject 12 added, with no way for a no-JS
        // page to explain why the submission bounced.
        let html = render_list(&[], &fake_caller("admin"), None);
        assert!(html.contains(r#"name="scope_kind" value="all""#));
        assert!(html.contains(r#"name="scope_kind" value="target""#));
        assert!(html.contains(r#"name="scope_kind" value="tag""#));
    }

    #[test]
    fn create_form_embeds_csrf_token_as_hidden_field_when_present() {
        let html = render_list(&[], &fake_caller("admin"), Some("tok123"));
        assert!(html.contains(r#"<input type="hidden" name="csrf_token" value="tok123">"#));
    }

    #[test]
    fn create_form_omits_csrf_field_when_absent() {
        let html = render_list(&[], &fake_caller("admin"), None);
        assert!(!html.contains(r#"name="csrf_token""#));
    }

    #[test]
    fn create_form_escapes_csrf_token() {
        let html = render_list(&[], &fake_caller("admin"), Some("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains(r#"value="<script>""#));
    }

    #[test]
    fn member_sees_no_create_form_at_all() {
        let html = render_list(&[], &fake_caller("member"), Some("tok123"));
        assert!(!html.contains("<form"));
        assert!(!html.contains("csrf_token"));
    }
}
