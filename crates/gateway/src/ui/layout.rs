//! Common page chrome and shared HTML helpers.
//!
//! [`wrap`] renders the standard HTML shell every authenticated page sits
//! inside: a header with three verb-grouped navigation columns
//! (見る / 直す / 証明する), a `<main>` region, and a footer. Component
//! helpers (`card`, `status_badge`, `metric_card`, etc.) live in the
//! sibling [`components`] module; CSS lives in [`style`]; WCAG contrast
//! verification lives in [`contrast`].
//!
//! ## ABDD baseline
//!
//! Every authenticated page receives, by default:
//!
//! - A skip link to `#main-content`.
//! - Semantic landmark roles (`banner`, `navigation`, `main`,
//!   `contentinfo`).
//! - An `aria-current="page"` on whichever nav link matches the active
//!   page title (the matching is done in [`active_route_for_title`] —
//!   pure logic, unit-tested).
//! - Three logical nav groups labelled "Observe" / "Operate" / "Verify"
//!   so each link's purpose is communicated to assistive tech.
//! - The CSRF token in a `<meta name="csrf-token">` for browser-side
//!   fetch helpers.
//!
//! ## Navigation grouping
//!
//! Links are grouped by the verb the user is performing, mirroring the
//! UI/UX guidance:
//!
//! | Group | Items | Purpose |
//! |---|---|---|
//! | **Observe** | Dashboard, Incidents, Stats | "Look at things — what's going on?" |
//! | **Operate** | Targets, Channels, Maintenance | "Change things — what needs adjusting?" |
//! | **Verify** | Audit, Settings, Migration (admin) | "Prove things — who did what, and how do we recover?" |
//!
//! `/me/security` lives in the user-info chip (top-right), not in the
//! main nav, because it is a per-user concern rather than a workspace
//! one. `/auth/logout` is in the same chip.

pub mod components;
pub mod contrast;
pub mod style;

use noye_shared::Caller;

use style::CSS;

// ──────────────────────────────────────────────────────────────────
//  Re-exports for backwards compatibility
//
//  Existing pages call `layout::escape_html(...)`,
//  `layout::status_badge(...)`, `layout::relative_time(...)` directly.
//  Re-export them from `components` so those call sites keep working
//  without change.
// ──────────────────────────────────────────────────────────────────

// Re-exports for backwards compatibility. Existing pages call these via
// `layout::escape_html`, `layout::status_badge`, `layout::relative_time`;
// new pages can also reach for the additional component helpers via
// `layout::card`, `layout::metric_card`, etc. Some are not yet used by
// any caller (later UI/UX phases will adopt them); the allow keeps the
// build clean during rollout.
#[allow(unused_imports)]
pub use components::{
    BadgeKind, ButtonKind, MetricTone, ResultTone, Tab, card, escape_html, inline_result,
    metric_card, relative_time, status_badge_from_code as status_badge, tabs, time_local,
};

/// Identify which top-level nav route is active for a given page title.
///
/// Page modules pass the page title to `wrap()` (e.g. `"Dashboard"`,
/// `"Targets"`); the same title is then the source of truth for
/// `aria-current="page"`. We translate the title to a route via this
/// pure function, which is easy to unit-test and makes the mapping
/// auditable.
///
/// Returns the nav `href` string that should be marked active, or
/// `None` if the page is not represented in the main nav (e.g. the
/// per-user `Security` page sits in the user chip instead).
pub fn active_route_for_title(title: &str) -> Option<&'static str> {
    match title {
        "Dashboard" => Some("/"),
        "Targets" => Some("/targets"),
        // Detail pages reuse the parent's nav-active state.
        t if t.starts_with("Target: ") => Some("/targets"),
        "Incidents" => Some("/incidents"),
        "Maintenance" => Some("/maintenance"),
        "Notification channels" => Some("/channels"),
        t if t.starts_with("Channel: ") => Some("/channels"),
        "Stats" => Some("/stats"),
        t if t.starts_with("Stats: ") => Some("/stats"),
        "Audit Log" => Some("/audit"),
        "Settings" => Some("/settings"),
        "Configuration migration" => Some("/admin/migration"),
        _ => None,
    }
}

/// Build the rendered HTML for one nav link, marking it active when the
/// current page's `active_route` matches.
fn nav_link(href: &str, label: &str, active_route: Option<&str>) -> String {
    let current_attr = if active_route == Some(href) {
        r#" aria-current="page""#
    } else {
        ""
    };
    format!(
        r#"<li><a href="{href}"{current_attr}>{label}</a></li>"#,
        href = escape_html(href),
        label = escape_html(label),
    )
}

/// Render the three verb-grouped nav columns. Admin-only items are only
/// included when the caller has the admin role.
fn render_nav(active_route: Option<&str>, is_admin: bool) -> String {
    // Observe — "what's going on right now?"
    let observe = format!(
        r#"<div class="nav-group" aria-labelledby="nav-observe-label">
  <span class="nav-group-label" id="nav-observe-label">Observe</span>
  <ul>{home}{incidents}{stats}</ul>
</div>"#,
        home = nav_link("/", "Dashboard", active_route),
        incidents = nav_link("/incidents", "Incidents", active_route),
        stats = nav_link("/stats", "Stats", active_route),
    );

    // Operate — "what needs adjusting?"
    let operate = format!(
        r#"<div class="nav-group" aria-labelledby="nav-operate-label">
  <span class="nav-group-label" id="nav-operate-label">Operate</span>
  <ul>{targets}{channels}{maint}</ul>
</div>"#,
        targets = nav_link("/targets", "Targets", active_route),
        channels = nav_link("/channels", "Channels", active_route),
        maint = nav_link("/maintenance", "Maintenance", active_route),
    );

    // Verify — admin-only group; if the caller is not admin, omit it
    // entirely so members never see the heading either.
    let verify = if is_admin {
        format!(
            r#"<div class="nav-group" aria-labelledby="nav-verify-label">
  <span class="nav-group-label" id="nav-verify-label">Verify</span>
  <ul>{audit}{settings}{migration}</ul>
</div>"#,
            audit = nav_link("/audit", "Audit", active_route),
            settings = nav_link("/settings", "Settings", active_route),
            migration = nav_link("/admin/migration", "Migration", active_route),
        )
    } else {
        String::new()
    };

    format!(
        r#"<nav role="navigation" aria-label="Main navigation">{observe}{operate}{verify}</nav>"#
    )
}

/// Render the user chip on the right of the header — name, role badge,
/// link to `/me/security`, and logout.
fn render_user_info(caller: &Caller) -> String {
    format!(
        r#"<div class="user-info" aria-label="Account">
  <span>{name}</span>
  <span class="role-badge" aria-label="Role: {role}">{role}</span>
  <a href="/me/security" aria-label="Account security">Security</a>
  <a href="/auth/logout" aria-label="Log out">Log out</a>
</div>"#,
        name = escape_html(&caller.name),
        role = escape_html(&caller.role),
    )
}

/// Wrap a piece of body content in the standard page chrome.
///
/// `csrf_token` surfaces the session's anti-CSRF token via a
/// `<meta name="csrf-token">` tag for browser-side fetch helpers. `None`
/// is acceptable for a legacy session (pre-CSRF rollout) — those callers
/// will not be able to make state-changing requests until they re-login,
/// and `verify_csrf` has a corresponding allow-once path.
pub fn wrap(title: &str, caller: &Caller, csrf_token: Option<&str>, content: &str) -> String {
    let csrf_meta = match csrf_token {
        Some(t) => format!(r#"<meta name="csrf-token" content="{}">"#, escape_html(t)),
        None => String::new(),
    };
    let active_route = active_route_for_title(title);
    let nav = render_nav(active_route, caller.is_admin());
    let user_info = render_user_info(caller);

    format!(
        r##"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    {csrf_meta}
    <title>{title} — Noye Monitor</title>
    <style>{CSS}</style>
</head>
<body>
    <a href="#main-content" class="skip-link">Skip to main content</a>

    <header role="banner">
        <div class="header-inner">
            <h1 class="logo">
                <a href="/" aria-label="Noye Monitor home">Noye</a>
            </h1>
            {nav}
            {user_info}
        </div>
    </header>

    <main id="main-content" role="main" tabindex="-1">
        <div class="container">
            <h2 class="page-title">{title}</h2>
            {content}
        </div>
    </main>

    <footer role="contentinfo">
        <p>Noye Monitor — Accessible by Default and by Design</p>
    </footer>
</body>
</html>"##,
        title = escape_html(title),
        CSS = CSS,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn admin() -> Caller {
        Caller {
            user_id: "u1".into(),
            email: "admin@example.com".into(),
            name: "Admin User".into(),
            role: "admin".into(),
        }
    }
    fn member() -> Caller {
        Caller {
            user_id: "u2".into(),
            email: "member@example.com".into(),
            name: "Member User".into(),
            role: "member".into(),
        }
    }

    // ── active_route_for_title ──

    #[test]
    fn active_route_maps_dashboard_and_top_pages() {
        assert_eq!(active_route_for_title("Dashboard"), Some("/"));
        assert_eq!(active_route_for_title("Targets"), Some("/targets"));
        assert_eq!(active_route_for_title("Incidents"), Some("/incidents"));
        assert_eq!(active_route_for_title("Stats"), Some("/stats"));
        assert_eq!(active_route_for_title("Audit Log"), Some("/audit"));
        assert_eq!(active_route_for_title("Settings"), Some("/settings"));
        assert_eq!(
            active_route_for_title("Configuration migration"),
            Some("/admin/migration")
        );
    }

    #[test]
    fn active_route_handles_detail_pages() {
        // Detail pages reuse the parent's nav-active state.
        assert_eq!(active_route_for_title("Target: web-01"), Some("/targets"));
        assert_eq!(
            active_route_for_title("Channel: Slack Ops"),
            Some("/channels")
        );
        assert_eq!(active_route_for_title("Stats: web-01"), Some("/stats"));
    }

    #[test]
    fn active_route_returns_none_for_chip_pages() {
        // /me/security lives in the user chip and intentionally does
        // not light up a nav group.
        assert_eq!(active_route_for_title("Security"), None);
        assert_eq!(active_route_for_title("Account security"), None);
    }

    #[test]
    fn active_route_returns_none_for_unknown_titles() {
        assert_eq!(active_route_for_title(""), None);
        assert_eq!(active_route_for_title("Unknown"), None);
    }

    // ── render_nav ──

    #[test]
    fn nav_groups_are_present_for_admin() {
        let html = render_nav(Some("/"), true);
        assert!(html.contains("nav-observe-label"));
        assert!(html.contains("nav-operate-label"));
        assert!(html.contains("nav-verify-label"));
    }

    #[test]
    fn nav_omits_verify_group_for_member() {
        let html = render_nav(Some("/"), false);
        assert!(html.contains("nav-observe-label"));
        assert!(html.contains("nav-operate-label"));
        assert!(!html.contains("nav-verify-label"));
        // None of the admin-only links should appear.
        assert!(!html.contains(r#"href="/audit""#));
        assert!(!html.contains(r#"href="/settings""#));
        assert!(!html.contains(r#"href="/admin/migration""#));
    }

    #[test]
    fn nav_marks_only_one_link_active() {
        let html = render_nav(Some("/targets"), true);
        let count = html.matches("aria-current=").count();
        assert_eq!(count, 1, "expected exactly one aria-current marker");
        assert!(html.contains(r#"<a href="/targets" aria-current="page">Targets</a>"#));
    }

    #[test]
    fn nav_marks_no_link_when_route_is_none() {
        let html = render_nav(None, true);
        assert!(!html.contains("aria-current"));
    }

    // ── render_user_info ──

    #[test]
    fn user_info_includes_security_and_logout() {
        let html = render_user_info(&admin());
        assert!(html.contains(r#"href="/me/security""#));
        assert!(html.contains(r#"href="/auth/logout""#));
        assert!(html.contains(">admin<"));
        assert!(html.contains("Admin User"));
    }

    #[test]
    fn user_info_escapes_caller_name() {
        let mut c = admin();
        c.name = "<script>".into();
        let html = render_user_info(&c);
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    // ── wrap ──

    #[test]
    fn wrap_emits_skip_link_and_main_landmark() {
        let html = wrap("Dashboard", &admin(), None, "<p>body</p>");
        assert!(html.contains(r##"href="#main-content""##));
        assert!(html.contains(r#"id="main-content""#));
        assert!(html.contains(r#"role="main""#));
        assert!(html.contains(r#"role="banner""#));
        assert!(html.contains(r#"role="contentinfo""#));
    }

    #[test]
    fn wrap_includes_csrf_meta_when_token_present() {
        let html = wrap("Dashboard", &admin(), Some("abc123"), "");
        assert!(html.contains(r#"<meta name="csrf-token" content="abc123">"#));
    }

    #[test]
    fn wrap_omits_csrf_meta_when_token_absent() {
        let html = wrap("Dashboard", &admin(), None, "");
        assert!(!html.contains("csrf-token"));
    }

    #[test]
    fn wrap_marks_active_nav_for_known_title() {
        let html = wrap("Targets", &admin(), None, "");
        assert!(html.contains(r#"<a href="/targets" aria-current="page">Targets</a>"#));
    }

    #[test]
    fn wrap_does_not_mark_active_for_unknown_title() {
        let html = wrap("Custom Page", &admin(), None, "");
        // No nav link receives aria-current, even though the page is rendered.
        // (The CSS itself contains `[aria-current=...]` attribute selectors,
        // so we look for the attribute as it would appear on an <a> tag,
        // not just the bare string.)
        assert!(!html.contains(r#" aria-current="page""#));
    }

    #[test]
    fn wrap_admin_sees_verify_group_member_does_not() {
        let admin_html = wrap("Dashboard", &admin(), None, "");
        let member_html = wrap("Dashboard", &member(), None, "");
        assert!(admin_html.contains("nav-verify-label"));
        assert!(!member_html.contains("nav-verify-label"));
    }

    #[test]
    fn wrap_escapes_title() {
        let html = wrap("<script>", &admin(), None, "");
        assert!(!html.contains("<title><script>"));
        assert!(html.contains("&lt;script&gt;"));
    }

    #[test]
    fn wrap_passes_body_through_unescaped() {
        // The body is server-rendered HTML; helpers escaped it already.
        let html = wrap("Dashboard", &admin(), None, "<div class=\"x\">body</div>");
        assert!(html.contains(r#"<div class="x">body</div>"#));
    }
}
