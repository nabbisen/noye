//! Targets pages — list and tabbed detail.
//!
//! ## Phase C re-design
//!
//! The detail page is split into four tabs (URL-driven, no JavaScript):
//!
//! | Tab | Content |
//! |---|---|
//! | **Overview** (default) | Status badge, plus "what would the next check look like" — host, port, path, expected_status, protocol-specific help |
//! | **Results** | Recent check results table |
//! | **Channels** | Attached notification channels (the existing `render_target_attachments` block, hosted here as a sibling tab) |
//! | **Settings** | Target metadata: timeout, retry count, interval, tags, owner |
//!
//! The active tab is determined by `?tab=...` in the URL. Unknown values
//! fall back to Overview. Each tab is `<a href="...">` so reloading the
//! page keeps the tab selection (bookmarkable, no client state to lose).
//!
//! ## Why split into tabs
//!
//! The old single-page layout asked the operator to scroll through a
//! mixture of "what is this target?", "is it healthy right now?", and
//! "who gets paged?" — three different questions that rarely need the
//! same answer at the same time. Each tab is a concrete question.
//!
//! ## Why no edit form yet
//!
//! Targets are still managed via API (`POST /api/targets` etc.) — the
//! list page surfaces this in an Add Target accordion and the Settings
//! tab here points at the same API. Adding a full edit form is on the
//! roadmap; Phase C scope is the read-side restructuring.

use noye_shared::{Caller, CheckResult, Target, TargetState};

use crate::ui::layout::{Tab, card, escape_html, status_badge, tabs, time_local};

// ──────────────────────────────────────────────────────────────────
//  Tab enum + parser (pure logic, unit-tested)
// ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetTab {
    Overview,
    Results,
    Channels,
    Settings,
}

impl TargetTab {
    /// Stable URL-query value for the tab. Used in `?tab=...` links.
    pub fn slug(self) -> &'static str {
        match self {
            TargetTab::Overview => "overview",
            TargetTab::Results => "results",
            TargetTab::Channels => "channels",
            TargetTab::Settings => "settings",
        }
    }

    /// Human-readable label for the tab navigation.
    pub fn label(self) -> &'static str {
        match self {
            TargetTab::Overview => "Overview",
            TargetTab::Results => "Recent results",
            TargetTab::Channels => "Notifications",
            TargetTab::Settings => "Settings",
        }
    }

    /// Parse a slug from the URL. Unknown values return [`TargetTab::Overview`]
    /// so a malformed bookmark still renders something useful.
    pub fn parse(s: &str) -> Self {
        match s {
            "results" => TargetTab::Results,
            "channels" => TargetTab::Channels,
            "settings" => TargetTab::Settings,
            _ => TargetTab::Overview,
        }
    }

    /// Iteration order for the tab strip. Overview first because it is
    /// the default; Settings last because it's least frequently visited.
    pub fn all() -> &'static [Self] {
        &[
            Self::Overview,
            Self::Results,
            Self::Channels,
            Self::Settings,
        ]
    }
}

/// Per-protocol help text shown on the Overview tab.
///
/// Returns the explanation specific to the target's `type` — what
/// "expected_status" means for HTTP, what TCP "host:port" is checking,
/// and so on. Returns `None` for an unknown type so the page can omit
/// the help block entirely rather than showing something misleading.
pub fn protocol_help(target_type: &str) -> Option<&'static str> {
    match target_type {
        "http" | "https" => Some(
            "HTTP(S) check sends a GET request and matches the response status against expected_status. body_contains, when set, requires the substring to appear in the response body for the check to pass.",
        ),
        "tcp" => Some(
            "TCP check opens a connection to host:port. The check passes when the TCP handshake completes within the timeout; no application-layer data is exchanged.",
        ),
        "smtp" => Some(
            "SMTP check opens a connection and waits for the server greeting (220 banner). Valid for ports 25, 465, 587 — it does not authenticate or send mail.",
        ),
        "tls" => Some(
            "TLS check completes a TLS handshake and inspects the certificate. The check fails when the certificate's days-to-expiry drops below tls_threshold_days.",
        ),
        _ => None,
    }
}

// ──────────────────────────────────────────────────────────────────
//  List page
// ──────────────────────────────────────────────────────────────────

pub fn render_list(targets: &[Target], states: &[TargetState], caller: &Caller) -> String {
    let mut html = String::new();

    if caller.is_admin() {
        let body = r#"<details>
  <summary><strong>+ Add new target</strong></summary>
  <div style="margin-top:var(--space-md)">
    <p>Create new targets via the API:</p>
    <p><code>POST /api/targets</code> with a JSON body. See <a href="/admin/migration">Configuration migration</a> to bulk-import targets, or the <a href="https://github.com/" target="_blank" rel="noopener">project docs</a> for the schema.</p>
  </div>
</details>"#;
        html.push_str(&card("Manage targets", "targets-manage", body));
    }

    if targets.is_empty() {
        html.push_str(&card(
            "No targets",
            "targets-empty",
            r#"<p role="status">No targets are registered.</p>"#,
        ));
        return html;
    }

    let mut body = String::new();
    body.push_str(r#"<table aria-label="Targets">"#);
    body.push_str("<thead><tr>");
    body.push_str(r#"<th scope="col">Status</th>"#);
    body.push_str(r#"<th scope="col">Name</th>"#);
    body.push_str(r#"<th scope="col">Type</th>"#);
    body.push_str(r#"<th scope="col">Host</th>"#);
    body.push_str(r#"<th scope="col">Interval</th>"#);
    body.push_str(r#"<th scope="col">Last check</th>"#);
    body.push_str("</tr></thead><tbody>");

    for target in targets {
        let state = states.iter().find(|s| s.target_id == target.id);
        let status = state
            .map(|s| s.current_status.as_str())
            .unwrap_or("unknown");
        let last_checked = state
            .and_then(|s| s.last_checked_at.as_deref())
            .unwrap_or("—");

        let row_attrs = if target.is_disabled {
            r#" class="row-disabled" aria-disabled="true""#
        } else {
            ""
        };

        body.push_str(&format!("<tr{}>", row_attrs));
        body.push_str(&format!("<td>{}</td>", status_badge(status)));
        body.push_str(&format!(
            r#"<td><a href="/targets/{id}">{name}</a>{disabled}</td>"#,
            id = escape_html(&target.id),
            name = escape_html(&target.name),
            disabled = if target.is_disabled {
                r#" <span class="badge badge-unknown">disabled</span>"#
            } else {
                ""
            },
        ));
        body.push_str(&format!(
            r#"<td><span class="badge badge-info">{}</span></td>"#,
            escape_html(&target.target_type)
        ));
        body.push_str(&format!(
            "<td>{}{}</td>",
            escape_html(&target.host),
            target.port.map(|p| format!(":{}", p)).unwrap_or_default()
        ));
        body.push_str(&format!("<td>{}m</td>", target.interval_minutes));
        body.push_str(&format!(
            "<td>{}</td>",
            if last_checked == "—" {
                "—".to_string()
            } else {
                time_local(last_checked)
            }
        ));
        body.push_str("</tr>");
    }
    body.push_str("</tbody></table>");

    html.push_str(&card("All targets", "targets-list", &body));
    html
}

// ──────────────────────────────────────────────────────────────────
//  Detail page (tabbed)
// ──────────────────────────────────────────────────────────────────

/// Render the targets/:id page with a tab strip + the active tab's body.
///
/// `active` controls which tab is highlighted and which body is rendered.
/// Channel-related rendering still lives in `ui::channels` (called from
/// the gateway handler when `active == Channels`); this function returns
/// only the chrome for the Channels tab and the handler appends the
/// channel block.
///
/// The tabs themselves are pure links — `?tab=overview|results|channels|
/// settings` on the same path. No JavaScript involved.
pub fn render_detail(
    target: &Target,
    state: &TargetState,
    results: &[CheckResult],
    active: TargetTab,
) -> String {
    let mut html = String::new();
    html.push_str(&render_header(target, state));
    html.push_str(&render_tab_strip(&target.id, active));
    match active {
        TargetTab::Overview => html.push_str(&render_overview(target, state)),
        TargetTab::Results => html.push_str(&render_results(results)),
        TargetTab::Channels => html.push_str(&render_channels_placeholder()),
        TargetTab::Settings => html.push_str(&render_settings(target)),
    }
    html
}

/// Compact header that's always visible regardless of which tab is
/// active. The status badge is the most important bit at a glance, so
/// it sits above the tabs.
fn render_header(target: &Target, state: &TargetState) -> String {
    let body = format!(
        r#"<dl class="info-grid">
  <dt>Status</dt><dd>{status}</dd>
  <dt>Type</dt><dd><span class="badge badge-info">{type_}</span></dd>
  <dt>Host</dt><dd><code>{host}{port}{path}</code></dd>
  <dt>Last check</dt><dd>{last}</dd>
</dl>"#,
        status = status_badge(&state.current_status),
        type_ = escape_html(&target.target_type),
        host = escape_html(&target.host),
        port = target.port.map(|p| format!(":{}", p)).unwrap_or_default(),
        path = escape_html(target.path.as_deref().unwrap_or("")),
        last = state
            .last_checked_at
            .as_deref()
            .map(time_local)
            .unwrap_or_else(|| "—".to_string()),
    );
    card("At a glance", "target-glance", &body)
}

fn render_tab_strip(target_id: &str, active: TargetTab) -> String {
    let id_html = escape_html(target_id);
    let entries: Vec<(TargetTab, String)> = TargetTab::all()
        .iter()
        .map(|t| (*t, format!("/targets/{}?tab={}", id_html, t.slug())))
        .collect();
    let active_idx = entries.iter().position(|(t, _)| *t == active).unwrap_or(0);
    let tab_items: Vec<Tab<'_>> = entries
        .iter()
        .map(|(t, href)| Tab {
            href,
            label: t.label(),
        })
        .collect();
    tabs(&tab_items, active_idx, "Target sections")
}

fn render_overview(target: &Target, state: &TargetState) -> String {
    // Protocol-specific help (when known). Hide rather than guess.
    let help_section = match protocol_help(&target.target_type) {
        Some(text) => format!(
            r#"<p style="margin-top:var(--space-md)"><strong>About this check.</strong> {}</p>"#,
            escape_html(text)
        ),
        None => String::new(),
    };

    // Show the bits relevant to "what is being checked" — the body of
    // the request, the success criteria, and the current consecutive
    // counters that drive transitions.
    let mut rows = String::new();
    rows.push_str(&dl_pair(
        "Expected status",
        &target
            .expected_status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "200".to_string()),
    ));
    if let Some(ref body) = target.body_contains {
        rows.push_str(&dl_pair("Body must contain", &escape_html(body)));
    }
    if let Some(threshold) = target.tls_threshold_days {
        rows.push_str(&dl_pair("TLS threshold", &format!("{} days", threshold)));
    }
    rows.push_str(&dl_pair(
        "Consecutive successes",
        &state.consecutive_successes.to_string(),
    ));
    rows.push_str(&dl_pair(
        "Consecutive failures",
        &state.consecutive_failures.to_string(),
    ));

    let body = format!(
        r#"<dl class="info-grid">{rows}</dl>{help}"#,
        rows = rows,
        help = help_section,
    );
    card("Overview", "target-overview", &body)
}

fn render_results(results: &[CheckResult]) -> String {
    if results.is_empty() {
        return card(
            "Recent results",
            "target-results",
            r#"<p>No check results yet. The next monitor run will record one.</p>"#,
        );
    }

    let mut body = String::new();
    body.push_str(r#"<table aria-label="recent check results">"#);
    body.push_str(r#"<thead><tr><th scope="col">Result</th><th scope="col">Status</th><th scope="col">Response time</th><th scope="col">Checked at</th><th scope="col">Error</th></tr></thead>"#);
    body.push_str("<tbody>");
    for result in results {
        let result_badge = if result.is_success {
            r#"<span class="badge badge-up" role="status" aria-label="Success">OK</span>"#
        } else {
            r#"<span class="badge badge-down" role="status" aria-label="Failure">FAIL</span>"#
        };
        body.push_str("<tr>");
        body.push_str(&format!("<td>{}</td>", result_badge));
        body.push_str(&format!(
            "<td>{}</td>",
            result
                .status_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "—".to_string())
        ));
        body.push_str(&format!(
            "<td>{}ms</td>",
            result.response_time_ms.unwrap_or(0)
        ));
        body.push_str(&format!("<td>{}</td>", time_local(&result.checked_at)));
        body.push_str(&format!(
            "<td>{}</td>",
            escape_html(result.error_message.as_deref().unwrap_or("—"))
        ));
        body.push_str("</tr>");
    }
    body.push_str("</tbody></table>");
    card("Recent results", "target-results", &body)
}

fn render_channels_placeholder() -> String {
    // The actual channel attachments are appended by the gateway handler
    // (it already has the data from `core_client::list_channels_for_target`).
    // This placeholder is the tab-content marker so the page structure is
    // consistent — the handler concatenates the attachment block right
    // after this card's heading.
    card(
        "Notification channels",
        "target-channels",
        r#"<p>Channels attached to this target are listed below. Manage attachments here; the channel itself is configured in <a href="/channels">Channels</a>.</p>"#,
    )
}

fn render_settings(target: &Target) -> String {
    let mut rows = String::new();
    rows.push_str(&dl_pair("Timeout", &format!("{}s", target.timeout_sec)));
    rows.push_str(&dl_pair(
        "Retries per check",
        &target.retry_count.to_string(),
    ));
    rows.push_str(&dl_pair(
        "Check interval",
        &format!("{} minute(s)", target.interval_minutes),
    ));
    rows.push_str(&dl_pair("Owner", &escape_html(&target.owner_id)));
    if let Some(ref tags) = target.tags {
        rows.push_str(&dl_pair("Tags", &escape_html(tags)));
    }
    rows.push_str(&dl_pair(
        "Disabled",
        if target.is_disabled { "yes" } else { "no" },
    ));

    let body = format!(
        r#"<dl class="info-grid">{rows}</dl>
<p style="margin-top:var(--space-md)">Settings are read-only here. Update via <code>PUT /api/targets/{id}</code> or use <a href="/admin/migration">Configuration migration</a>.</p>"#,
        rows = rows,
        id = escape_html(&target.id),
    );
    card("Settings", "target-settings", &body)
}

fn dl_pair(label: &str, value: &str) -> String {
    format!(
        "<dt>{}</dt><dd>{}</dd>",
        escape_html(label),
        value, // value is rendered HTML — caller pre-escapes as needed
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_target(id: &str, name: &str, ttype: &str) -> Target {
        Target {
            id: id.into(),
            name: name.into(),
            target_type: ttype.into(),
            host: "example.com".into(),
            port: Some(443),
            path: Some("/health".into()),
            expected_status: Some(200),
            body_contains: None,
            tls_threshold_days: None,
            timeout_sec: 10,
            retry_count: 3,
            interval_minutes: 5,
            is_disabled: false,
            owner_id: "user-1".into(),
            tags: None,
            next_check_at: "2026-04-29T10:05:00Z".into(),
            created_at: "2026-04-01T00:00:00Z".into(),
            updated_at: "2026-04-01T00:00:00Z".into(),
        }
    }

    fn fake_state(target_id: &str, status: &str) -> TargetState {
        TargetState {
            target_id: target_id.into(),
            current_status: status.into(),
            consecutive_successes: 12,
            consecutive_failures: 0,
            success_threshold: 3,
            failure_threshold: 3,
            last_checked_at: Some("2026-04-29T10:00:00Z".into()),
            last_status_change_at: None,
            last_notification_at: None,
        }
    }

    fn fake_result(success: bool) -> CheckResult {
        CheckResult {
            id: "r1".into(),
            target_id: "t1".into(),
            checked_at: "2026-04-29T10:00:00Z".into(),
            is_success: success,
            status_code: if success { Some(200) } else { Some(500) },
            response_time_ms: Some(45),
            error_message: if success {
                None
            } else {
                Some("upstream 500".into())
            },
            tls_expiry_date: None,
            tls_days_left: None,
            details: None,
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

    // ── TargetTab ──

    #[test]
    fn tab_slug_and_label_are_distinct() {
        let mut slugs = std::collections::HashSet::new();
        let mut labels = std::collections::HashSet::new();
        for t in TargetTab::all() {
            assert!(slugs.insert(t.slug()), "duplicate slug: {}", t.slug());
            assert!(labels.insert(t.label()), "duplicate label: {}", t.label());
        }
        assert_eq!(slugs.len(), 4);
    }

    #[test]
    fn tab_parse_round_trips() {
        for t in TargetTab::all() {
            assert_eq!(TargetTab::parse(t.slug()), *t);
        }
    }

    #[test]
    fn tab_parse_unknown_falls_back_to_overview() {
        assert_eq!(TargetTab::parse(""), TargetTab::Overview);
        assert_eq!(TargetTab::parse("garbage"), TargetTab::Overview);
        assert_eq!(TargetTab::parse("OVERVIEW"), TargetTab::Overview);
    }

    #[test]
    fn tab_all_orders_overview_first_settings_last() {
        let all = TargetTab::all();
        assert_eq!(all.first(), Some(&TargetTab::Overview));
        assert_eq!(all.last(), Some(&TargetTab::Settings));
    }

    // ── protocol_help ──

    #[test]
    fn protocol_help_known_types() {
        assert!(protocol_help("http").is_some());
        assert!(protocol_help("https").is_some());
        assert!(protocol_help("tcp").is_some());
        assert!(protocol_help("smtp").is_some());
        assert!(protocol_help("tls").is_some());
    }

    #[test]
    fn protocol_help_unknown_returns_none() {
        // The Overview tab omits the help block entirely when help is
        // None, rather than showing misleading generic text.
        assert!(protocol_help("").is_none());
        assert!(protocol_help("ws").is_none());
        assert!(protocol_help("HTTP").is_none()); // case-sensitive
    }

    #[test]
    fn protocol_help_text_mentions_relevant_concepts() {
        // We don't pin exact wording; we require keywords that confirm
        // the help text is talking about the right protocol.
        assert!(protocol_help("http").unwrap().contains("expected_status"));
        assert!(protocol_help("tcp").unwrap().contains("handshake"));
        assert!(protocol_help("tls").unwrap().contains("threshold"));
    }

    // ── render_list ──

    #[test]
    fn list_empty_admin_shows_add_new_block_and_empty_message() {
        let html = render_list(&[], &[], &fake_caller("admin"));
        assert!(html.contains("Manage targets"));
        assert!(html.contains("POST /api/targets"));
        assert!(html.contains("No targets are registered"));
    }

    #[test]
    fn list_empty_member_omits_add_new_block() {
        let html = render_list(&[], &[], &fake_caller("member"));
        // Members aren't allowed to create targets so the management
        // accordion should not render for them.
        assert!(!html.contains("Manage targets"));
        assert!(!html.contains("POST /api/targets"));
        assert!(html.contains("No targets are registered"));
    }

    #[test]
    fn list_renders_target_rows_with_status_badges() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let html = render_list(&[t], &[s], &fake_caller("admin"));
        assert!(html.contains("badge-up"));
        assert!(html.contains(r#"href="/targets/t1""#));
        assert!(html.contains("web-01"));
    }

    #[test]
    fn list_marks_disabled_targets_with_aria_disabled() {
        let mut t = fake_target("t1", "web-01", "https");
        t.is_disabled = true;
        let s = fake_state("t1", "unknown");
        let html = render_list(&[t], &[s], &fake_caller("admin"));
        assert!(html.contains(r#"aria-disabled="true""#));
        assert!(html.contains(">disabled<"));
    }

    // ── render_detail ──

    #[test]
    fn detail_renders_tab_strip_with_aria_current_on_active() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let html = render_detail(&t, &s, &[], TargetTab::Results);
        // Exactly one nav link is current; that link is the Results tab.
        assert_eq!(html.matches(r#"aria-current="page""#).count(), 1);
        assert!(html.contains(
            r#"<a href="/targets/t1?tab=results" aria-current="page">Recent results</a>"#
        ));
    }

    #[test]
    fn detail_overview_includes_protocol_help() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let html = render_detail(&t, &s, &[], TargetTab::Overview);
        // The "About this check" help paragraph appears for known
        // protocols.
        assert!(html.contains("About this check"));
    }

    #[test]
    fn detail_overview_omits_help_for_unknown_protocol() {
        let mut t = fake_target("t1", "web-01", "ws");
        t.target_type = "ws".into();
        let s = fake_state("t1", "unknown");
        let html = render_detail(&t, &s, &[], TargetTab::Overview);
        assert!(!html.contains("About this check"));
    }

    #[test]
    fn detail_results_tab_renders_table_when_results_present() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let r = vec![fake_result(true), fake_result(false)];
        let html = render_detail(&t, &s, &r, TargetTab::Results);
        assert!(html.contains("OK"));
        assert!(html.contains("FAIL"));
        assert!(html.contains("upstream 500"));
    }

    #[test]
    fn detail_results_tab_friendly_when_empty() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "unknown");
        let html = render_detail(&t, &s, &[], TargetTab::Results);
        assert!(html.contains("No check results yet"));
        assert!(!html.contains("<table"));
    }

    #[test]
    fn detail_settings_shows_immutable_metadata() {
        let mut t = fake_target("t1", "web-01", "https");
        t.tags = Some(r#"["prod","critical"]"#.into());
        let s = fake_state("t1", "up");
        let html = render_detail(&t, &s, &[], TargetTab::Settings);
        assert!(html.contains("Timeout"));
        assert!(html.contains("Retries per check"));
        assert!(html.contains("Check interval"));
        assert!(html.contains("Owner"));
        // Tags appear when set.
        assert!(html.contains("[&quot;prod&quot;,&quot;critical&quot;]"));
        // Settings tab links to the API for updates.
        assert!(html.contains("PUT /api/targets/t1"));
    }

    #[test]
    fn detail_settings_omits_tags_row_when_unset() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let html = render_detail(&t, &s, &[], TargetTab::Settings);
        assert!(!html.contains(">Tags<"));
    }

    #[test]
    fn detail_channels_tab_renders_placeholder() {
        // The actual attachment block is appended by the gateway handler
        // (it has the channel data); this tab just supplies the heading.
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let html = render_detail(&t, &s, &[], TargetTab::Channels);
        assert!(html.contains("Notification channels"));
        // Help-text link to the channels page.
        assert!(html.contains(r#"href="/channels""#));
    }

    #[test]
    fn detail_at_a_glance_header_visible_on_every_tab() {
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        for tab in TargetTab::all() {
            let html = render_detail(&t, &s, &[], *tab);
            assert!(html.contains("At a glance"), "tab {tab:?} missing header");
        }
    }

    #[test]
    fn detail_unknown_tab_falls_back_via_parse() {
        // Confirms the parse-and-render loop: unknown query → Overview.
        // We can't pass the string directly to render_detail (it takes
        // TargetTab), so we exercise it through TargetTab::parse.
        let parsed = TargetTab::parse("nope");
        let t = fake_target("t1", "web-01", "https");
        let s = fake_state("t1", "up");
        let html = render_detail(&t, &s, &[], parsed);
        // The Overview content is present.
        assert!(html.contains("Expected status"));
    }
}
