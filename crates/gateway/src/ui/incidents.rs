//! Incidents page — list, manual-resolve, and split-by-status.
//!
//! ## Phase B re-design
//!
//! The page is split into two visually distinct regions:
//!
//! - **Open** (top, prominent) — incidents that still need attention.
//! - **Resolved** (below, in a `<details>` element so the page doesn't
//!   bury the open ones) — recently-closed incidents for context.
//!
//! Manual resolution previously took a free-text `prompt()` for the note;
//! the rewritten flow uses a structured `<select>` with four pre-defined
//! reasons plus an optional free-text detail. The reason code is
//! prepended to the resolution note as `[reason_code] detail` (see
//! [`compose_resolution_note`]) so the audit log carries machine-parseable
//! provenance while still letting operators record specifics.
//!
//! ## Why structured reasons
//!
//! Every manual-resolve we've seen in practice falls into one of four
//! buckets — externally recovered, transient (already healthy by the time
//! someone looked), target removed (no longer applicable), or "other"
//! (anything that needs detail). A free-text-only flow leaves the audit
//! log inconsistent and makes the bucketing useless to anyone trying to
//! analyze incident trends afterward.
//!
//! ## Compatibility with the existing API
//!
//! Resolution still posts to `POST /api/incidents/:id/resolve` with the
//! shape `{note: string|null}`. The page-side JavaScript composes the
//! note from `[code] free-text`. The Core endpoint and audit-log shape
//! are unchanged, so old API clients (if any) keep working.

use noye_shared::Incident;

use crate::ui::layout::{
    card, escape_html, inline_result, status_badge, time_local, ResultTone,
};

// ──────────────────────────────────────────────────────────────────
//  Pure-logic helpers (testable without a worker runtime)
// ──────────────────────────────────────────────────────────────────

/// Split incidents into `(open, resolved)` preserving original order.
///
/// We don't sort here — the caller is expected to pass incidents in the
/// order they want them displayed. `partition` gives us the two slices
/// without forcing a clone of the whole list.
pub fn partition_incidents<'a>(incidents: &'a [Incident]) -> (Vec<&'a Incident>, Vec<&'a Incident>) {
    let mut open = Vec::new();
    let mut resolved = Vec::new();
    for inc in incidents {
        if inc.status == "open" {
            open.push(inc);
        } else {
            resolved.push(inc);
        }
    }
    (open, resolved)
}

/// Resolution-reason codes shown to the operator. The trailing `Other`
/// variant unlocks the free-text detail input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionReason {
    /// Operator confirmed the issue was resolved by some external action
    /// (e.g. a third party restarted the service).
    RecoveredExternally,
    /// The check failed transiently and is already healthy by the time
    /// the operator looked. No real outage.
    Transient,
    /// The target itself was removed/decommissioned, so the open
    /// incident should be closed administratively.
    TargetRemoved,
    /// Anything that doesn't fit the buckets above; the free-text detail
    /// is required.
    Other,
}

impl ResolutionReason {
    /// Reason code as recorded in the audit log. Stable string for
    /// machine-parseability after the fact.
    pub fn code(self) -> &'static str {
        match self {
            ResolutionReason::RecoveredExternally => "recovered_externally",
            ResolutionReason::Transient => "transient",
            ResolutionReason::TargetRemoved => "target_removed",
            ResolutionReason::Other => "other",
        }
    }

    /// Human-readable label for the dropdown.
    pub fn label(self) -> &'static str {
        match self {
            ResolutionReason::RecoveredExternally => "Recovered externally",
            ResolutionReason::Transient => "Transient — already healthy",
            ResolutionReason::TargetRemoved => "Target was removed",
            ResolutionReason::Other => "Other (specify below)",
        }
    }

    /// Parse a code string back to a reason. Returns `None` for unknown
    /// codes, so the caller can decide whether to reject or fall through
    /// to a default.
    ///
    /// Currently only used in unit tests; kept as a `pub` API entry
    /// point for future audit-log post-processing (decoding the
    /// `[code]` prefix back into a structured value).
    #[allow(dead_code)]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "recovered_externally" => Some(Self::RecoveredExternally),
            "transient" => Some(Self::Transient),
            "target_removed" => Some(Self::TargetRemoved),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// Iteration order for the `<select>` options.
    pub fn all() -> &'static [Self] {
        &[
            Self::RecoveredExternally,
            Self::Transient,
            Self::TargetRemoved,
            Self::Other,
        ]
    }
}

/// Compose the audit-log note from a reason code and an optional free
/// text. Format: `[code] detail` when detail is non-empty, `[code]`
/// otherwise.
///
/// The `[code]` prefix is the bridging contract between the structured
/// dropdown and the unstructured `notes` column on the existing API. As
/// long as both producers (the UI) and consumers (anyone querying the
/// audit log) follow this convention, the column stays parseable.
///
/// Currently only used in unit tests — the JavaScript on the dialog
/// produces the same string client-side. Kept as a `pub` server-side
/// helper so any future endpoint that wants to compose a resolution
/// programmatically (e.g. an admin CLI) can reuse the convention.
#[allow(dead_code)]
pub fn compose_resolution_note(code: &str, detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        format!("[{}]", code)
    } else {
        format!("[{}] {}", code, detail)
    }
}

/// Format the duration field on an incident row.
///
/// `None` is rendered as `—`, which is the visible state for "still
/// open." Open incidents have no resolved_at and therefore no
/// duration_sec; the `—` is correct semantically and the screen-reader
/// label clarifies it.
pub fn format_duration(seconds: Option<i64>) -> String {
    match seconds {
        Some(s) if s >= 86_400 => format!("{}d {}h", s / 86_400, (s % 86_400) / 3600),
        Some(s) if s >= 3600 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        Some(s) if s >= 60 => format!("{}m {}s", s / 60, s % 60),
        Some(s) => format!("{}s", s),
        None => "—".to_string(),
    }
}

// ──────────────────────────────────────────────────────────────────
//  Page rendering
// ──────────────────────────────────────────────────────────────────

/// Render the incidents list page. The caller is expected to pass
/// incidents in the desired display order (typically opened_at DESC).
pub fn render_list(incidents: &[Incident]) -> String {
    if incidents.is_empty() {
        return card(
            "Incidents",
            "incidents-empty",
            r#"<p role="status">There are no incidents.</p>"#,
        );
    }

    let (open, resolved) = partition_incidents(incidents);

    let mut html = String::new();
    html.push_str(&render_help_card());
    html.push_str(&render_open_card(&open));
    if !resolved.is_empty() {
        html.push_str(&render_resolved_details(&resolved));
    }
    // The resolve dialog and its script are only useful when there are
    // open incidents to act on. Skipping them when the page is fully
    // resolved keeps the markup smaller and the test for "no resolve UI
    // when nothing is open" trivially correct.
    if !open.is_empty() {
        html.push_str(&render_resolve_dialog());
        html.push_str(&render_script());
    }
    html
}

fn render_help_card() -> String {
    card(
        "How notifications and incidents differ",
        "incidents-help",
        r#"<p><strong>Notification</strong> happens once, on the up→down or down→up state change. <strong>Incidents</strong> remain in this list — open from the moment a target goes down until it's verified healthy again — even if no further notification fires. This separation lets you suppress notifications during maintenance without losing the audit record.</p>"#,
    )
}

fn render_open_card(open: &[&Incident]) -> String {
    if open.is_empty() {
        return card(
            "Open incidents",
            "incidents-open",
            r#"<p role="status">No open incidents.</p>"#,
        );
    }

    let mut body = String::new();
    body.push_str(&format!(
        r#"<p role="status" aria-live="polite">{} open incident{}.</p>"#,
        open.len(),
        if open.len() == 1 { "" } else { "s" }
    ));
    body.push_str(&render_incidents_table(open, true));
    card("Open incidents", "incidents-open", &body)
}

fn render_resolved_details(resolved: &[&Incident]) -> String {
    let body = format!(
        r#"<details>
  <summary>Recently resolved ({count})</summary>
  <div class="resolved-incidents">{table}</div>
</details>"#,
        count = resolved.len(),
        table = render_incidents_table(resolved, false),
    );
    card("Resolved", "incidents-resolved", &body)
}

fn render_incidents_table(incidents: &[&Incident], show_resolve_button: bool) -> String {
    let mut html = String::new();
    html.push_str(r#"<table>"#);
    html.push_str("<thead><tr>");
    html.push_str(r#"<th scope="col">Status</th>"#);
    html.push_str(r#"<th scope="col">Target</th>"#);
    html.push_str(r#"<th scope="col">Cause</th>"#);
    html.push_str(r#"<th scope="col">Opened</th>"#);
    if !show_resolve_button {
        html.push_str(r#"<th scope="col">Resolved</th>"#);
    }
    html.push_str(r#"<th scope="col">Duration</th>"#);
    if !show_resolve_button {
        html.push_str(r#"<th scope="col">Note</th>"#);
    } else {
        html.push_str(r#"<th scope="col">Action</th>"#);
    }
    html.push_str("</tr></thead><tbody>");

    for inc in incidents {
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", status_badge(&inc.status)));
        // Target is rendered as a link to the target's detail page.
        // Until the Incident shape carries `target_name` we show the id.
        html.push_str(&format!(
            r#"<td><a href="/targets/{id}">{id}</a></td>"#,
            id = escape_html(&inc.target_id),
        ));
        html.push_str(&format!(
            "<td>{}</td>",
            escape_html(inc.cause.as_deref().unwrap_or("—"))
        ));
        html.push_str(&format!("<td>{}</td>", time_local(&inc.opened_at)));
        if !show_resolve_button {
            html.push_str(&format!(
                "<td>{}</td>",
                inc.resolved_at
                    .as_deref()
                    .map(time_local)
                    .unwrap_or_else(|| "—".to_string())
            ));
        }
        html.push_str(&format!("<td>{}</td>", format_duration(inc.duration_sec)));
        if show_resolve_button {
            html.push_str(&format!(
                r#"<td><button type="button" class="btn btn-sm btn-secondary action-resolve" data-incident-id="{id}" data-target-id="{tid}">Resolve…</button></td>"#,
                id = escape_html(&inc.id),
                tid = escape_html(&inc.target_id),
            ));
        } else {
            html.push_str(&format!(
                "<td>{}</td>",
                escape_html(inc.resolution_note.as_deref().unwrap_or("—"))
            ));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    html
}

/// Render the manual-resolve dialog (a `<dialog>` element triggered by
/// the row-level "Resolve…" button).
fn render_resolve_dialog() -> String {
    let options: String = ResolutionReason::all()
        .iter()
        .map(|r| {
            format!(
                r#"<option value="{code}">{label}</option>"#,
                code = r.code(),
                label = escape_html(r.label()),
            )
        })
        .collect();

    format!(
        r#"<dialog id="resolve-dialog" aria-labelledby="resolve-dialog-title">
  <form method="dialog" id="resolve-form">
    <h3 id="resolve-dialog-title">Manually resolve incident</h3>
    <p>Target: <strong id="resolve-target"></strong></p>
    <div class="field">
      <label for="resolve-reason">Reason</label>
      <select id="resolve-reason" name="reason" required>{options}</select>
    </div>
    <div class="field">
      <label for="resolve-detail">Detail <span class="field-help">(optional unless reason is "Other")</span></label>
      <textarea id="resolve-detail" name="detail" rows="3"></textarea>
    </div>
    {result}
    <div class="form-actions">
      <button type="submit" value="confirm" class="btn btn-primary">Resolve incident</button>
      <button type="button" class="btn btn-ghost" id="resolve-cancel">Cancel</button>
    </div>
  </form>
</dialog>"#,
        result = inline_result("resolve-result", ResultTone::Error),
        options = options,
    )
}

/// Inline JavaScript for the resolve dialog. Reads CSRF token from the
/// `<meta>` tag (rendered by `layout::wrap`), composes the note via the
/// `[code] detail` convention, and POSTs to the existing endpoint.
fn render_script() -> String {
    r#"<script>
(function () {
  const dialog = document.getElementById('resolve-dialog');
  const form = document.getElementById('resolve-form');
  const targetEl = document.getElementById('resolve-target');
  const reasonEl = document.getElementById('resolve-reason');
  const detailEl = document.getElementById('resolve-detail');
  const resultEl = document.getElementById('resolve-result');
  const cancelBtn = document.getElementById('resolve-cancel');
  let activeIncidentId = null;
  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';

  if (!dialog || typeof dialog.showModal !== 'function') {
    // Browser without <dialog> support: fall back to a plain confirm.
    document.querySelectorAll('.action-resolve').forEach((btn) => {
      btn.addEventListener('click', async () => {
        if (!confirm('Manually resolve this incident?')) return;
        const id = btn.dataset.incidentId;
        const headers = { 'Content-Type': 'application/json' };
        if (csrfToken) headers['X-CSRF-Token'] = csrfToken;
        const res = await fetch('/api/incidents/' + id + '/resolve', {
          method: 'POST', headers, body: JSON.stringify({ note: '[other]' }),
        });
        if (res.ok) location.reload();
        else alert('Resolution failed');
      });
    });
    return;
  }

  document.querySelectorAll('.action-resolve').forEach((btn) => {
    btn.addEventListener('click', () => {
      activeIncidentId = btn.dataset.incidentId;
      targetEl.textContent = btn.dataset.targetId;
      reasonEl.value = 'recovered_externally';
      detailEl.value = '';
      resultEl.hidden = true;
      resultEl.textContent = '';
      dialog.showModal();
      reasonEl.focus();
    });
  });

  cancelBtn.addEventListener('click', () => dialog.close());

  form.addEventListener('submit', async (ev) => {
    if (ev.submitter !== form.querySelector('button[type=submit]')) return;
    ev.preventDefault();
    if (!activeIncidentId) return;
    const reason = reasonEl.value;
    const detail = detailEl.value.trim();
    if (reason === 'other' && !detail) {
      resultEl.textContent = 'Please describe what happened.';
      resultEl.hidden = false;
      return;
    }
    const note = detail ? '[' + reason + '] ' + detail : '[' + reason + ']';
    const headers = { 'Content-Type': 'application/json' };
    if (csrfToken) headers['X-CSRF-Token'] = csrfToken;
    try {
      const res = await fetch('/api/incidents/' + activeIncidentId + '/resolve', {
        method: 'POST', headers, body: JSON.stringify({ note }),
      });
      if (!res.ok) throw new Error(await res.text());
      dialog.close();
      location.reload();
    } catch (e) {
      resultEl.textContent = 'Resolution failed: ' + e.message;
      resultEl.hidden = false;
    }
  });
})();
</script>"#
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_incident(id: &str, target: &str, status: &str) -> Incident {
        Incident {
            id: id.into(),
            target_id: target.into(),
            status: status.into(),
            opened_at: "2026-04-29T10:00:00Z".into(),
            resolved_at: if status == "resolved" {
                Some("2026-04-29T10:30:00Z".into())
            } else {
                None
            },
            duration_sec: if status == "resolved" { Some(1800) } else { None },
            cause: Some("HTTP 503".into()),
            resolution_note: if status == "resolved" {
                Some("[recovered_externally] DBA restarted the pool".into())
            } else {
                None
            },
            created_by: None,
        }
    }

    // ── partition_incidents ──

    #[test]
    fn partition_separates_open_and_resolved() {
        let list = vec![
            fake_incident("1", "t1", "open"),
            fake_incident("2", "t2", "resolved"),
            fake_incident("3", "t3", "open"),
        ];
        let (open, resolved) = partition_incidents(&list);
        assert_eq!(open.len(), 2);
        assert_eq!(resolved.len(), 1);
        assert_eq!(open[0].id, "1");
        assert_eq!(open[1].id, "3");
        assert_eq!(resolved[0].id, "2");
    }

    #[test]
    fn partition_preserves_original_order() {
        let list = vec![
            fake_incident("c", "t", "open"),
            fake_incident("a", "t", "open"),
            fake_incident("b", "t", "open"),
        ];
        let (open, _) = partition_incidents(&list);
        // No sort: order is "c", "a", "b" as supplied.
        let ids: Vec<&str> = open.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    #[test]
    fn partition_handles_empty_input() {
        let (o, r) = partition_incidents(&[]);
        assert!(o.is_empty());
        assert!(r.is_empty());
    }

    #[test]
    fn partition_handles_unknown_status_as_resolved() {
        // Defensive: any status string other than "open" lands in the
        // resolved bucket. This matches the page's intent ("don't bury
        // anything in the dropdown that isn't visibly broken").
        let list = vec![
            fake_incident("1", "t", "open"),
            fake_incident("2", "t", "weird"),
        ];
        let (o, r) = partition_incidents(&list);
        assert_eq!(o.len(), 1);
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].id, "2");
    }

    // ── ResolutionReason ──

    #[test]
    fn reason_codes_are_stable_strings() {
        // The codes are stored verbatim in the audit log; if any of these
        // values change, existing log queries break.
        assert_eq!(
            ResolutionReason::RecoveredExternally.code(),
            "recovered_externally"
        );
        assert_eq!(ResolutionReason::Transient.code(), "transient");
        assert_eq!(ResolutionReason::TargetRemoved.code(), "target_removed");
        assert_eq!(ResolutionReason::Other.code(), "other");
    }

    #[test]
    fn reason_from_code_round_trips() {
        for r in ResolutionReason::all() {
            assert_eq!(ResolutionReason::from_code(r.code()), Some(*r));
        }
    }

    #[test]
    fn reason_from_code_rejects_unknown() {
        assert_eq!(ResolutionReason::from_code(""), None);
        assert_eq!(ResolutionReason::from_code("RECOVERED_EXTERNALLY"), None);
        assert_eq!(ResolutionReason::from_code("garbage"), None);
    }

    #[test]
    fn reason_all_yields_ordered_buckets() {
        // The `all()` order drives the dropdown order; "Other" must be
        // last because the form unlocks the free-text only when the user
        // explicitly chose Other.
        let codes: Vec<&str> = ResolutionReason::all().iter().map(|r| r.code()).collect();
        assert_eq!(
            codes,
            vec!["recovered_externally", "transient", "target_removed", "other"]
        );
    }

    // ── compose_resolution_note ──

    #[test]
    fn compose_resolution_note_with_detail() {
        let s = compose_resolution_note("transient", "DNS blip, recovered in 30s");
        assert_eq!(s, "[transient] DNS blip, recovered in 30s");
    }

    #[test]
    fn compose_resolution_note_without_detail() {
        let s = compose_resolution_note("recovered_externally", "");
        assert_eq!(s, "[recovered_externally]");
    }

    #[test]
    fn compose_resolution_note_strips_surrounding_whitespace() {
        // Operators sometimes paste whitespace by accident. The leading
        // marker should still be `[code]` with no leading space inside
        // the body.
        let s = compose_resolution_note("other", "   moved off-prem  ");
        assert_eq!(s, "[other] moved off-prem");
    }

    #[test]
    fn compose_resolution_note_empty_detail_after_trim() {
        let s = compose_resolution_note("transient", "   ");
        assert_eq!(s, "[transient]");
    }

    // ── format_duration ──

    #[test]
    fn format_duration_em_dash_for_none() {
        assert_eq!(format_duration(None), "—");
    }

    #[test]
    fn format_duration_seconds() {
        assert_eq!(format_duration(Some(0)), "0s");
        assert_eq!(format_duration(Some(45)), "45s");
        assert_eq!(format_duration(Some(59)), "59s");
    }

    #[test]
    fn format_duration_minutes() {
        assert_eq!(format_duration(Some(60)), "1m 0s");
        assert_eq!(format_duration(Some(125)), "2m 5s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(Some(3600)), "1h 0m");
        assert_eq!(format_duration(Some(3725)), "1h 2m");
    }

    #[test]
    fn format_duration_days() {
        assert_eq!(format_duration(Some(86_400)), "1d 0h");
        assert_eq!(format_duration(Some(90_000)), "1d 1h");
    }

    // ── render_list ──

    #[test]
    fn render_list_empty_shows_friendly_message() {
        let html = render_list(&[]);
        assert!(html.contains("There are no incidents"));
        assert!(html.contains(r#"role="status""#));
    }

    #[test]
    fn render_list_open_section_has_role_status() {
        let list = vec![fake_incident("1", "web-01", "open")];
        let html = render_list(&list);
        assert!(html.contains(r#"role="status""#));
        assert!(html.contains("1 open incident"));
    }

    #[test]
    fn render_list_with_only_resolved_omits_resolve_button_column() {
        let list = vec![fake_incident("1", "web-01", "resolved")];
        let html = render_list(&list);
        // The resolved table doesn't render the action column.
        assert!(!html.contains("action-resolve"));
        // It does include the resolution note column.
        assert!(html.contains("DBA restarted the pool"));
    }

    #[test]
    fn render_list_with_open_renders_resolve_button_with_data_attrs() {
        let list = vec![fake_incident("inc-9", "web-01", "open")];
        let html = render_list(&list);
        assert!(html.contains("action-resolve"));
        assert!(html.contains(r#"data-incident-id="inc-9""#));
        assert!(html.contains(r#"data-target-id="web-01""#));
    }

    #[test]
    fn render_list_target_id_links_to_detail() {
        let list = vec![fake_incident("1", "web-01", "open")];
        let html = render_list(&list);
        assert!(html.contains(r#"href="/targets/web-01""#));
    }

    #[test]
    fn render_list_resolve_dialog_lists_all_reasons() {
        let list = vec![fake_incident("1", "web-01", "open")];
        let html = render_list(&list);
        for r in ResolutionReason::all() {
            let want = format!(r#"value="{}""#, r.code());
            assert!(
                html.contains(&want),
                "missing reason option: {}",
                r.code()
            );
        }
    }

    #[test]
    fn render_list_help_card_explains_notification_vs_incident() {
        let list = vec![fake_incident("1", "web-01", "open")];
        let html = render_list(&list);
        // The help text differentiates the concepts; we don't pin the
        // wording but require both keywords to appear.
        assert!(html.to_lowercase().contains("notification"));
        assert!(html.to_lowercase().contains("incident"));
    }

    #[test]
    fn render_list_open_and_resolved_split_into_distinct_cards() {
        let list = vec![
            fake_incident("1", "web-01", "open"),
            fake_incident("2", "db-01", "resolved"),
        ];
        let html = render_list(&list);
        assert!(html.contains(r#"id="incidents-open""#));
        assert!(html.contains(r#"id="incidents-resolved""#));
        // The resolved section is collapsed under <details>.
        assert!(html.contains("<details>"));
    }

    #[test]
    fn render_list_escapes_cause_text() {
        let mut inc = fake_incident("1", "web-01", "open");
        inc.cause = Some("<script>alert(1)</script>".into());
        let html = render_list(&[inc]);
        assert!(!html.contains("<script>alert(1)</script>"));
        assert!(html.contains("&lt;script&gt;"));
    }
}
