//! Shared HTML component helpers.
//!
//! Every component is a pure function returning a `String`. Pages compose
//! pages by stitching component output together. No DOM manipulation, no
//! template engine — we lean on Rust's `format!` macro and the type system
//! to keep the surface small.
//!
//! ## Why pure functions
//!
//! - **Unit-testable.** We can pin the exact HTML each helper emits without
//!   spinning up a worker runtime, a browser, or a network. Tests assert
//!   that the relevant attributes (`role`, `aria-*`, semantic structure)
//!   appear in output.
//! - **No state.** The helpers don't read or mutate global state, so they
//!   compose freely.
//! - **No I/O.** They run anywhere a `String` can be returned — including
//!   in tests on the host target where there is no `worker::*`.
//!
//! ## ABDD baked in
//!
//! Each component's defaults emit ARIA roles and labels appropriate for
//! that pattern (e.g. `status_badge` includes `role="status"` and an
//! accessible `aria-label`; `tabs` uses `aria-current="page"` for the
//! active link). Page authors don't have to remember to add them.

// Some components defined here are first used in subsequent UI/UX phases
// (B, C, D). They are tested in this module and re-exported, but the
// existing pages don't yet call them; suppress the unused-warning for
// the duration of the rollout.
#![allow(dead_code)]

// ──────────────────────────────────────────────────────────────────
//  HTML escaping
// ──────────────────────────────────────────────────────────────────

/// Escape `&`, `<`, `>`, `"`, and `'` for safe inclusion in HTML.
///
/// This is the single place in the UI that performs HTML escaping. Every
/// untrusted string flowing into the page passes through here. Forgetting
/// to escape user input is the most common XSS vector, so component
/// helpers always escape their inputs themselves rather than trusting
/// callers to remember.
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// ──────────────────────────────────────────────────────────────────
//  Time formatting
// ──────────────────────────────────────────────────────────────────

/// Render a `<time>` element. The browser surfaces the text content as-is
/// (so server-rendered ISO strings remain readable when JavaScript is off)
/// and the `datetime` attribute lets assistive tech read the value
/// unambiguously.
pub fn time_local(timestamp: &str) -> String {
    let escaped = escape_html(timestamp);
    format!(r#"<time datetime="{escaped}">{escaped}</time>"#)
}

/// Backwards-compatible alias for [`time_local`].
///
/// The earlier name was `relative_time` even though it didn't actually
/// render a relative format. Existing pages use it; we keep the alias so
/// they don't break, and switch to `time_local` in new code.
pub fn relative_time(timestamp: &str) -> String {
    time_local(timestamp)
}

// ──────────────────────────────────────────────────────────────────
//  Status badge
// ──────────────────────────────────────────────────────────────────

/// Visual badge kinds. Maps to a `--c-*-bg` / `--c-*` token pair.
///
/// We use an enum (rather than free strings) so the page-level callers
/// can't accidentally pass an unknown status string and silently render
/// the "unknown" fallback. Conversion from a status string lives in
/// [`BadgeKind::from_state`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BadgeKind {
    Up,
    Down,
    Degraded,
    Maintenance,
    Unknown,
    Info,
}

impl BadgeKind {
    fn css_class(self) -> &'static str {
        match self {
            BadgeKind::Up => "badge-up",
            BadgeKind::Down => "badge-down",
            BadgeKind::Degraded => "badge-degraded",
            BadgeKind::Maintenance => "badge-maint",
            BadgeKind::Unknown => "badge-unknown",
            BadgeKind::Info => "badge-info",
        }
    }

    /// Map a status code from D1 (or anywhere else) to a badge kind.
    ///
    /// Unknown codes return `Unknown` so the page renders something
    /// sensible rather than panicking.
    pub fn from_state(code: &str) -> Self {
        match code {
            "up" => BadgeKind::Up,
            "down" => BadgeKind::Down,
            "degraded" => BadgeKind::Degraded,
            "maintenance" => BadgeKind::Maintenance,
            "open" => BadgeKind::Down,
            "resolved" => BadgeKind::Up,
            "acknowledged" => BadgeKind::Degraded,
            _ => BadgeKind::Unknown,
        }
    }
}

/// Render a badge — `kind` controls the visual; `label` is the visible
/// text and is also used as the accessible name. The dot prefix is added
/// by CSS via `::before`, so screen readers do not announce it.
pub fn status_badge(kind: BadgeKind, label: &str) -> String {
    let class = kind.css_class();
    let label_html = escape_html(label);
    format!(
        r#"<span class="badge {class}" role="status" aria-label="{label_html}">{label_html}</span>"#
    )
}

/// Convenience: render a badge directly from a status code, using the
/// code itself as the visible label.
///
/// Used by existing pages that pass through D1 status strings literally;
/// new pages should prefer `status_badge(BadgeKind, label)` with a
/// human-friendly label.
pub fn status_badge_from_code(status: &str) -> String {
    let kind = BadgeKind::from_state(status);
    let label = match status {
        "up" => "Up",
        "down" => "Down",
        "degraded" => "Degraded",
        "maintenance" => "Maintenance",
        "open" => "Open",
        "resolved" => "Resolved",
        "acknowledged" => "Acknowledged",
        "unknown" => "Unknown",
        other => other,
    };
    status_badge(kind, label)
}

// ──────────────────────────────────────────────────────────────────
//  Card
// ──────────────────────────────────────────────────────────────────

/// Wrap content in a labelled card.
///
/// The heading id derived from `id_hint` lets the card be referenced via
/// `aria-labelledby` for tooling that wants to scope its readout. When
/// `id_hint` is empty we omit the id and the heading instead receives a
/// stable but non-referenceable id-less rendering.
pub fn card(title: &str, id_hint: &str, body: &str) -> String {
    if id_hint.is_empty() {
        format!(
            r#"<section class="card"><h3>{title}</h3>{body}</section>"#,
            title = escape_html(title),
            body = body,
        )
    } else {
        let id = escape_html(id_hint);
        format!(
            r#"<section class="card" aria-labelledby="{id}"><h3 id="{id}">{title}</h3>{body}</section>"#,
            id = id,
            title = escape_html(title),
            body = body,
        )
    }
}

// ──────────────────────────────────────────────────────────────────
//  Metric card (Dashboard / summary panels)
// ──────────────────────────────────────────────────────────────────

/// One large value with a small label and optional hint underneath.
///
/// Setting `tone` to a non-default kind tints the value (e.g. green for
/// "all up", red for "down count > 0"). `hint` is rendered as muted
/// secondary text — typical use is "12 of 14 online" under a value of
/// "14".
pub fn metric_card(label: &str, value: &str, hint: Option<&str>, tone: MetricTone) -> String {
    let class = match tone {
        MetricTone::Default => "metric-card",
        MetricTone::Up => "metric-card up",
        MetricTone::Down => "metric-card down",
        MetricTone::Degraded => "metric-card degraded",
    };
    let hint_html = match hint {
        Some(h) if !h.is_empty() => {
            format!(r#"<div class="metric-hint">{}</div>"#, escape_html(h))
        }
        _ => String::new(),
    };
    format!(
        r#"<div class="{class}">
  <div class="metric-label">{label}</div>
  <div class="metric-value">{value}</div>
  {hint_html}
</div>"#,
        label = escape_html(label),
        value = escape_html(value),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricTone {
    Default,
    Up,
    Down,
    Degraded,
}

// ──────────────────────────────────────────────────────────────────
//  Tabs
// ──────────────────────────────────────────────────────────────────

/// A tab. `href` becomes the `<a>` target and `label` is the visible
/// text. The active tab is identified by index in [`tabs`].
#[derive(Debug, Clone)]
pub struct Tab<'a> {
    pub href: &'a str,
    pub label: &'a str,
}

/// Render an `<nav>` of tabs. The `active` index identifies which tab
/// receives `aria-current="page"`. An out-of-range index simply means no
/// tab is currently active (defensive — shouldn't happen in practice).
pub fn tabs(tabs: &[Tab<'_>], active: usize, aria_label: &str) -> String {
    let items: String = tabs
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let current = if i == active { r#" aria-current="page""# } else { "" };
            format!(
                r#"<a href="{href}"{current}>{label}</a>"#,
                href = escape_html(t.href),
                label = escape_html(t.label),
            )
        })
        .collect();
    format!(
        r#"<nav class="tabs" aria-label="{aria}">{items}</nav>"#,
        aria = escape_html(aria_label),
        items = items,
    )
}

// ──────────────────────────────────────────────────────────────────
//  Inline result panel
// ──────────────────────────────────────────────────────────────────

/// Severity classes for inline result panels. See CSS section 10.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTone {
    Success,
    Error,
    Warn,
    Info,
}

impl ResultTone {
    fn css_class(self) -> &'static str {
        match self {
            ResultTone::Success => "success",
            ResultTone::Error => "error",
            ResultTone::Warn => "warn",
            ResultTone::Info => "info",
        }
    }
}

/// Render a hidden inline-result element. JavaScript fills the message
/// and toggles `hidden` on action results (e.g. test-send).
///
/// Pages typically render this once, near the action that produces it,
/// and assign it a stable `id` so client code can find it without
/// querying class names. `aria-live="polite"` ensures the message is
/// announced when it appears.
pub fn inline_result(id: &str, tone: ResultTone) -> String {
    format!(
        r#"<output id="{id}" class="inline-result {tone}" role="status" aria-live="polite" hidden></output>"#,
        id = escape_html(id),
        tone = tone.css_class(),
    )
}

// ──────────────────────────────────────────────────────────────────
//  Button (style helper, no behaviour)
// ──────────────────────────────────────────────────────────────────

/// Button-class names. The component is a thin presentation wrapper —
/// behaviour (onclick handlers, form submission, etc.) is the caller's
/// responsibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Secondary,
    Ghost,
    Danger,
}

impl ButtonKind {
    pub fn css_class(self) -> &'static str {
        match self {
            ButtonKind::Primary => "btn btn-primary",
            ButtonKind::Secondary => "btn btn-secondary",
            ButtonKind::Ghost => "btn btn-ghost",
            ButtonKind::Danger => "btn btn-danger",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_html_handles_all_five_entities() {
        let s = escape_html(r#"<a href="x">'&'</a>"#);
        assert!(!s.contains('<'));
        assert!(!s.contains('>'));
        assert!(!s.contains('\''));
        assert!(s.contains("&amp;"));
        assert!(s.contains("&lt;"));
        assert!(s.contains("&gt;"));
        assert!(s.contains("&quot;"));
        assert!(s.contains("&#x27;"));
    }

    #[test]
    fn time_local_emits_time_element() {
        let h = time_local("2026-04-29T10:30:00Z");
        assert!(h.starts_with("<time "));
        assert!(h.contains(r#"datetime="2026-04-29T10:30:00Z""#));
        assert!(h.contains(">2026-04-29T10:30:00Z<"));
    }

    #[test]
    fn time_local_escapes_input() {
        // Defense-in-depth: the timestamp comes from D1 and shouldn't
        // contain HTML, but we don't trust it.
        let h = time_local("<script>");
        assert!(!h.contains("<script>"));
        assert!(h.contains("&lt;script&gt;"));
    }

    #[test]
    fn badge_kind_from_state_maps_known_codes() {
        assert_eq!(BadgeKind::from_state("up"), BadgeKind::Up);
        assert_eq!(BadgeKind::from_state("down"), BadgeKind::Down);
        assert_eq!(BadgeKind::from_state("degraded"), BadgeKind::Degraded);
        assert_eq!(BadgeKind::from_state("maintenance"), BadgeKind::Maintenance);
        assert_eq!(BadgeKind::from_state("open"), BadgeKind::Down);
        assert_eq!(BadgeKind::from_state("resolved"), BadgeKind::Up);
        assert_eq!(BadgeKind::from_state("acknowledged"), BadgeKind::Degraded);
    }

    #[test]
    fn badge_kind_from_state_falls_back_for_unknown() {
        assert_eq!(BadgeKind::from_state("garbage"), BadgeKind::Unknown);
        assert_eq!(BadgeKind::from_state(""), BadgeKind::Unknown);
    }

    #[test]
    fn status_badge_includes_role_and_label() {
        let h = status_badge(BadgeKind::Up, "Operational");
        assert!(h.contains(r#"role="status""#));
        assert!(h.contains(r#"aria-label="Operational""#));
        assert!(h.contains("badge-up"));
        assert!(h.contains(">Operational<"));
    }

    #[test]
    fn status_badge_escapes_label() {
        let h = status_badge(BadgeKind::Up, r#"x"y"#);
        assert!(h.contains("&quot;"));
        assert!(!h.contains(r#""y"#)); // raw escape sequence shouldn't survive
    }

    #[test]
    fn status_badge_from_code_uses_human_label() {
        // Even though the status code is "up", the visible label is "Up"
        // (capitalised). The aria-label matches.
        let h = status_badge_from_code("up");
        assert!(h.contains(">Up<"));
        assert!(h.contains(r#"aria-label="Up""#));
    }

    #[test]
    fn card_with_id_hint_uses_aria_labelledby() {
        let h = card("My Section", "my-section", "<p>body</p>");
        assert!(h.contains(r#"aria-labelledby="my-section""#));
        assert!(h.contains(r#"id="my-section""#));
        assert!(h.contains("<h3"));
        assert!(h.contains("My Section"));
        assert!(h.contains("<p>body</p>"));
    }

    #[test]
    fn card_without_id_hint_omits_aria() {
        let h = card("Plain", "", "body");
        assert!(!h.contains("aria-labelledby"));
        assert!(!h.contains(" id="));
        assert!(h.contains("Plain"));
    }

    #[test]
    fn card_escapes_title_but_not_body() {
        // The body is trusted (caller-rendered HTML); the title is not.
        let h = card("<title>", "x", "<div>raw</div>");
        assert!(h.contains("&lt;title&gt;"));
        assert!(h.contains("<div>raw</div>"));
    }

    #[test]
    fn metric_card_renders_label_value_and_hint() {
        let h = metric_card("Targets", "42", Some("12 up / 30 down"), MetricTone::Default);
        assert!(h.contains(">Targets<"));
        assert!(h.contains(">42<"));
        assert!(h.contains("12 up / 30 down"));
        assert!(h.contains(r#"class="metric-card""#));
    }

    #[test]
    fn metric_card_omits_hint_when_none() {
        let h = metric_card("Targets", "42", None, MetricTone::Default);
        assert!(!h.contains("metric-hint"));
    }

    #[test]
    fn metric_card_applies_tone_class() {
        let h = metric_card("X", "1", None, MetricTone::Up);
        assert!(h.contains(r#"class="metric-card up""#));
        let h = metric_card("X", "1", None, MetricTone::Down);
        assert!(h.contains(r#"class="metric-card down""#));
    }

    #[test]
    fn tabs_marks_active_index() {
        let t = vec![
            Tab { href: "/a", label: "A" },
            Tab { href: "/b", label: "B" },
            Tab { href: "/c", label: "C" },
        ];
        let h = tabs(&t, 1, "Sections");
        // Only the active tab has aria-current.
        let occurrences = h.matches("aria-current=").count();
        assert_eq!(occurrences, 1);
        // And it sits on the second tab.
        assert!(h.contains(r#"<a href="/b" aria-current="page">B</a>"#));
        // The aria-label is on the wrapping nav.
        assert!(h.contains(r#"aria-label="Sections""#));
    }

    #[test]
    fn tabs_with_out_of_range_active_renders_no_current() {
        let t = vec![Tab { href: "/a", label: "A" }];
        let h = tabs(&t, 99, "Out");
        assert!(!h.contains("aria-current"));
    }

    #[test]
    fn tabs_escapes_href_and_label() {
        let t = vec![Tab { href: r#"x"y"#, label: "<b>" }];
        let h = tabs(&t, 0, "X");
        assert!(h.contains("&quot;"));
        assert!(h.contains("&lt;b&gt;"));
    }

    #[test]
    fn inline_result_starts_hidden_and_polite() {
        let h = inline_result("save-result", ResultTone::Success);
        assert!(h.contains("hidden"));
        assert!(h.contains(r#"aria-live="polite""#));
        assert!(h.contains(r#"role="status""#));
        assert!(h.contains(r#"id="save-result""#));
        assert!(h.contains("inline-result success"));
    }

    #[test]
    fn inline_result_tone_classes_distinct() {
        assert!(inline_result("a", ResultTone::Success).contains("success"));
        assert!(inline_result("a", ResultTone::Error).contains("error"));
        assert!(inline_result("a", ResultTone::Warn).contains("warn"));
        assert!(inline_result("a", ResultTone::Info).contains("info"));
    }

    #[test]
    fn button_kind_classes_are_distinct() {
        // Sanity check that the four ButtonKind variants don't collide.
        let classes = [
            ButtonKind::Primary.css_class(),
            ButtonKind::Secondary.css_class(),
            ButtonKind::Ghost.css_class(),
            ButtonKind::Danger.css_class(),
        ];
        let unique: std::collections::HashSet<_> = classes.iter().copied().collect();
        assert_eq!(unique.len(), 4);
    }
}
