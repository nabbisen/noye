//! Audit log viewer page.
//!
//! ## Phase D re-design
//!
//! The page surfaces the audit log entries with their full provenance
//! (previous_value / new_value), but folds the JSON blobs into
//! `<details>` so the row stays readable. A small intro card explains
//! how to verify the hash chain (the actual verify button lives on
//! `/me/security` for admins, since that's where audit-log integrity
//! is most likely to be checked alongside personal-session sanity).
//!
//! ## What the audit log carries
//!
//! Every mutating action (target/channel/maintenance create/update/
//! delete, user upsert, manual incident resolve, login event) is
//! appended here with a tamper-evident hash chain. The Verify page
//! walks the chain end-to-end and reports any rows whose hash doesn't
//! match the recomputation.

use noye_shared::AuditEntry;

use crate::ui::layout::{card, escape_html, time_local};

/// Map the action_type string to a short glyph + label combination,
/// purely cosmetic. We keep this pure so the visual mapping is testable.
///
/// For unknown action types we return `("other", "other")` — the row's
/// raw `action_type` is still rendered in the table cell (we only use
/// this helper for the badge label), so no information is lost.
///
/// Currently exercised in unit tests; kept as a `pub` API entry point
/// for future use (e.g. an aggregations page that buckets events by
/// label rather than the raw type string).
#[allow(dead_code)]
pub fn action_label(action_type: &str) -> (&'static str, &'static str) {
    match action_type {
        "create" => ("create", "create"),
        "update" => ("update", "update"),
        "delete" => ("delete", "delete"),
        "login" => ("login", "login"),
        "import" => ("import", "import"),
        "resolve" => ("resolve", "resolve"),
        _ => ("other", "other"),
    }
}

/// Render the audit log list page. Caller passes entries in display
/// order (typically `action_time` DESC).
pub fn render_list(entries: &[AuditEntry]) -> String {
    let mut html = String::new();
    html.push_str(&render_intro_card());

    if entries.is_empty() {
        html.push_str(&card(
            "No entries",
            "audit-empty",
            r#"<p role="status">No audit log entries are recorded yet. Mutations to targets, channels, maintenance windows, users, and incidents will appear here.</p>"#,
        ));
        return html;
    }

    let mut body = String::new();
    body.push_str(r#"<table aria-label="Audit log entries">"#);
    body.push_str("<thead><tr>");
    body.push_str(r#"<th scope="col">Time</th>"#);
    body.push_str(r#"<th scope="col">Actor</th>"#);
    body.push_str(r#"<th scope="col">Action</th>"#);
    body.push_str(r#"<th scope="col">Resource</th>"#);
    body.push_str(r#"<th scope="col">Result</th>"#);
    body.push_str(r#"<th scope="col">IP</th>"#);
    body.push_str(r#"<th scope="col">Changes</th>"#);
    body.push_str("</tr></thead><tbody>");

    for e in entries {
        // We use action_label for the consistent set of recognised
        // events, but the cell shows the raw action_type so unknown
        // events (e.g. a future action) still surface meaningfully.
        let result_class = if e.result == "success" {
            "badge-up"
        } else {
            "badge-down"
        };

        body.push_str("<tr>");
        body.push_str(&format!("<td>{}</td>", time_local(&e.action_time)));
        body.push_str(&format!(
            "<td>{}</td>",
            escape_html(e.actor_email.as_deref().unwrap_or(&e.actor_id))
        ));
        body.push_str(&format!(
            r#"<td><span class="badge badge-info">{}</span></td>"#,
            escape_html(&e.action_type)
        ));
        body.push_str(&format!(
            "<td>{}: {}</td>",
            escape_html(&e.resource_type),
            escape_html(e.resource_id.as_deref().unwrap_or("—"))
        ));
        body.push_str(&format!(
            r#"<td><span class="badge {}">{}</span></td>"#,
            result_class,
            escape_html(&e.result)
        ));
        body.push_str(&format!(
            "<td>{}</td>",
            escape_html(e.ip_address.as_deref().unwrap_or("—"))
        ));
        body.push_str(&format!("<td>{}</td>", render_changes(e)));
        body.push_str("</tr>");
    }
    body.push_str("</tbody></table>");
    html.push_str(&card("All entries", "audit-list", &body));
    html
}

/// Render the previous/new value cells as a foldable `<details>`.
/// When the entry has neither, render an em-dash; we don't waste a
/// disclosure widget on rows that have nothing to disclose.
fn render_changes(e: &AuditEntry) -> String {
    let prev = e.previous_value.as_deref().unwrap_or("");
    let new = e.new_value.as_deref().unwrap_or("");
    if prev.is_empty() && new.is_empty() {
        return "—".to_string();
    }

    // Both fields, when present, are JSON strings written by the server.
    // The user can copy/inspect them; we just present them readably.
    let mut s = String::new();
    s.push_str(r#"<details><summary>Show diff</summary>"#);
    s.push_str(r#"<dl class="info-grid" style="margin-top:var(--space-xs)">"#);
    if !prev.is_empty() {
        s.push_str(&format!(
            r#"<dt>Previous</dt><dd><pre style="white-space:pre-wrap;font-size:var(--fs-xs);margin:0">{}</pre></dd>"#,
            escape_html(prev)
        ));
    }
    if !new.is_empty() {
        s.push_str(&format!(
            r#"<dt>New</dt><dd><pre style="white-space:pre-wrap;font-size:var(--fs-xs);margin:0">{}</pre></dd>"#,
            escape_html(new)
        ));
    }
    s.push_str("</dl>");
    s.push_str("</details>");
    s
}

fn render_intro_card() -> String {
    let body = r#"<p>Every mutating action — target / channel / maintenance window create-update-delete, user upsert, login, manual incident resolve, configuration import — is appended here. Each row carries a SHA-256 hash that links to the previous row, forming a tamper-evident chain.</p>
<p>To verify the chain end-to-end, open <a href="/me/security">Account security</a> and run <strong>Run integrity check</strong>. The verifier walks every row, recomputes its hash, and reports any tampered or out-of-order rows.</p>"#;
    card("How the audit log works", "audit-intro", body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_entry(id: &str, action: &str, result: &str) -> AuditEntry {
        AuditEntry {
            id: id.into(),
            action_time: "2026-04-29T10:00:00Z".into(),
            actor_id: "u-1".into(),
            actor_email: Some("alice@example.com".into()),
            resource_type: "target".into(),
            resource_id: Some("t-abc".into()),
            action_type: action.into(),
            previous_value: Some(r#"{"name":"old"}"#.into()),
            new_value: Some(r#"{"name":"new"}"#.into()),
            result: result.into(),
            ip_address: Some("203.0.113.5".into()),
        }
    }

    #[test]
    fn action_label_known_types() {
        assert_eq!(action_label("create").1, "create");
        assert_eq!(action_label("update").1, "update");
        assert_eq!(action_label("delete").1, "delete");
        assert_eq!(action_label("login").1, "login");
        assert_eq!(action_label("import").1, "import");
        assert_eq!(action_label("resolve").1, "resolve");
    }

    #[test]
    fn action_label_unknown_returns_other_label() {
        // Unknown action types produce ("other", "other"). The row
        // renderer shows the raw `action_type` in the cell so unknown
        // events still surface their actual name; this helper is only
        // used to derive a short fixed label for known buckets.
        assert_eq!(action_label("custom-event"), ("other", "other"));
    }

    #[test]
    fn list_empty_shows_friendly_message() {
        let html = render_list(&[]);
        assert!(html.contains(r#"role="status""#));
        assert!(html.contains("No audit log entries"));
        assert!(html.contains("Mutations to targets"));
    }

    #[test]
    fn list_intro_card_links_to_security_page() {
        // The hash-chain verify button lives on /me/security; the audit
        // page just points at it. We verify the link is present.
        let html = render_list(&[]);
        assert!(html.contains(r#"href="/me/security""#));
        assert!(html.contains("integrity check"));
    }

    #[test]
    fn list_renders_entry_with_action_badge_and_result_badge() {
        let e = fake_entry("a-1", "update", "success");
        let html = render_list(&[e]);
        assert!(html.contains(r#"<span class="badge badge-info">update</span>"#));
        assert!(html.contains(r#"<span class="badge badge-up">success</span>"#));
    }

    #[test]
    fn list_renders_failure_with_down_badge() {
        let e = fake_entry("a-1", "delete", "failure");
        let html = render_list(&[e]);
        assert!(html.contains(r#"<span class="badge badge-down">failure</span>"#));
    }

    #[test]
    fn list_actor_email_preferred_over_actor_id() {
        let e = fake_entry("a-1", "login", "success");
        let html = render_list(&[e]);
        assert!(html.contains("alice@example.com"));
        // The actor_id is only the fallback; here it shouldn't appear.
        assert!(!html.contains(">u-1<"));
    }

    #[test]
    fn list_actor_id_used_when_email_missing() {
        let mut e = fake_entry("a-1", "login", "success");
        e.actor_email = None;
        let html = render_list(&[e]);
        assert!(html.contains("u-1"));
    }

    #[test]
    fn list_changes_cell_uses_details_when_either_value_present() {
        let e = fake_entry("a-1", "update", "success");
        let html = render_list(&[e]);
        assert!(html.contains("<details>"));
        assert!(html.contains(">Show diff<"));
        // Both old and new values appear inside a <pre>.
        assert!(html.contains("<pre"));
        assert!(html.contains("&quot;name&quot;:&quot;old&quot;"));
        assert!(html.contains("&quot;name&quot;:&quot;new&quot;"));
    }

    #[test]
    fn list_changes_cell_em_dash_when_neither_value_present() {
        let mut e = fake_entry("a-1", "login", "success");
        e.previous_value = None;
        e.new_value = None;
        let html = render_list(&[e]);
        // No disclosure widget when there's nothing to disclose.
        assert!(!html.contains("Show diff"));
    }

    #[test]
    fn list_uses_time_element_for_action_time() {
        let e = fake_entry("a-1", "create", "success");
        let html = render_list(&[e]);
        assert!(html.contains(r#"<time datetime="2026-04-29T10:00:00Z""#));
    }

    #[test]
    fn list_resource_id_dash_when_missing() {
        let mut e = fake_entry("a-1", "login", "success");
        e.resource_id = None;
        let html = render_list(&[e]);
        // The cell renders "target: —" rather than swallowing the row.
        assert!(html.contains("target: —"));
    }

    #[test]
    fn list_ip_dash_when_missing() {
        let mut e = fake_entry("a-1", "create", "success");
        e.ip_address = None;
        let html = render_list(&[e]);
        // The IP cell falls back to em-dash.
        let count = html.matches("<td>—</td>").count();
        assert!(count >= 1, "expected at least one em-dash IP cell");
    }

    #[test]
    fn list_renders_unknown_action_type_verbatim() {
        // The renderer surfaces the raw action_type in the badge so
        // unknown events still tell the operator what happened.
        let mut e = fake_entry("a-1", "custom-event", "success");
        e.previous_value = None;
        e.new_value = None;
        let html = render_list(&[e]);
        assert!(html.contains(r#"<span class="badge badge-info">custom-event</span>"#));
    }

    #[test]
    fn list_escapes_actor_email() {
        let mut e = fake_entry("a-1", "create", "success");
        e.actor_email = Some("<script>".into());
        let html = render_list(&[e]);
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }
}
