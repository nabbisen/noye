//! Personal security page (`/me/security`).
//!
//! Shows the calling user the current state of their account from a
//! security-self-service angle: what session they're using right now, what
//! other sessions exist for the same email, recent login events from the
//! audit log, and (for admins) a one-click hash-chain integrity check.
//!
//! ## Why this lives separately from `/settings`
//!
//! `/settings` is admin-only and team-wide (user-management, integration
//! checks). `/me/security` is per-user and self-service. Splitting them
//! keeps the privilege boundary obvious: nothing on `/me/security` reveals
//! data about other users.

use noye_shared::{AuditEntry, Caller};

use crate::auth::session::Session;
use crate::ui::layout::{ResultTone, card, escape_html, inline_result, time_local};

/// Render the `/me/security` page body.
///
/// `current` is the session backing this very request (read from the
/// cookie); `all` is the full set of sessions (including `current`) for
/// the same `user_email`. The renderer derives "other sessions" itself so
/// the call site doesn't have to filter twice.
pub fn render(
    caller: &Caller,
    current: Option<&Session>,
    all: &[Session],
    login_history: &[AuditEntry],
    is_admin: bool,
) -> String {
    let mut html = String::new();

    html.push_str(&render_intro(caller));
    html.push_str(&render_current_session(current));
    html.push_str(&render_other_sessions(current, all));
    html.push_str(&render_login_history(login_history));
    if is_admin {
        html.push_str(&render_audit_verify_card());
    }
    html.push_str(&render_script());
    html
}

fn render_intro(caller: &Caller) -> String {
    let body = format!(
        r#"<dl class="info-grid">
  <dt>Email</dt><dd>{email}</dd>
  <dt>Display name</dt><dd>{name}</dd>
  <dt>Role</dt><dd>{role}</dd>
</dl>"#,
        email = escape_html(&caller.email),
        name = escape_html(&caller.name),
        role = escape_html(&caller.role),
    );
    card("Account", "me-intro", &body)
}

fn render_current_session(current: Option<&Session>) -> String {
    let body = match current {
        Some(s) => {
            let csrf_state = if s.csrf_token.is_some() {
                "active"
            } else {
                "legacy session (will refresh on next login)"
            };
            format!(
                r#"<dl class="info-grid">
  <dt>Issued</dt><dd>{}</dd>
  <dt>Expires</dt><dd>{}</dd>
  <dt>CSRF protection</dt><dd>{}</dd>
</dl>
<p style="margin-top:var(--space-md)"><a href="/auth/logout" class="action-link">Log out of this session</a></p>"#,
                format_unix_ts(s.issued_at),
                format_unix_ts(s.expires_at),
                csrf_state,
            )
        }
        None => r#"<p role="status">No active session detected.</p>"#.to_string(),
    };
    card("Current session", "me-current", &body)
}

fn render_other_sessions(current: Option<&Session>, all: &[Session]) -> String {
    let current_id = current.map(|s| s.session_id.as_str()).unwrap_or("");
    let others: Vec<&Session> = all.iter().filter(|s| s.session_id != current_id).collect();

    let body = if others.is_empty() {
        r#"<p role="status">No other active sessions.</p>"#.to_string()
    } else {
        // Sort by issued_at DESC so newest is first.
        let mut sorted = others.clone();
        sorted.sort_by(|a, b| b.issued_at.cmp(&a.issued_at));

        let mut s = String::new();
        s.push_str(r#"<table aria-label="other active sessions">"#);
        s.push_str(r#"<thead><tr>"#);
        s.push_str(r#"<th scope="col">Issued</th>"#);
        s.push_str(r#"<th scope="col">Expires</th>"#);
        s.push_str(r#"</tr></thead><tbody>"#);
        for sess in &sorted {
            s.push_str("<tr>");
            s.push_str(&format!("<td>{}</td>", format_unix_ts(sess.issued_at)));
            s.push_str(&format!("<td>{}</td>", format_unix_ts(sess.expires_at)));
            s.push_str("</tr>");
        }
        s.push_str("</tbody></table>");

        s.push_str(&format!(
            r#"<div class="form-actions">
  <button type="button" id="revoke-others" class="btn btn-danger">
    Log out of all other sessions
  </button>
</div>
{result}"#,
            result = inline_result("revoke-result", ResultTone::Info),
        ));
        s
    };
    card("Other sessions", "me-others", &body)
}

fn render_login_history(history: &[AuditEntry]) -> String {
    let body = if history.is_empty() {
        r#"<p role="status">No recent login events recorded for this account.</p>"#.to_string()
    } else {
        let mut s = String::new();
        s.push_str(r#"<table aria-label="recent logins">"#);
        s.push_str(r#"<thead><tr>"#);
        s.push_str(r#"<th scope="col">Time</th>"#);
        s.push_str(r#"<th scope="col">IP address</th>"#);
        s.push_str(r#"<th scope="col">Result</th>"#);
        s.push_str(r#"</tr></thead><tbody>"#);
        for e in history {
            s.push_str("<tr>");
            s.push_str(&format!("<td>{}</td>", time_local(&e.action_time)));
            s.push_str(&format!(
                "<td>{}</td>",
                escape_html(e.ip_address.as_deref().unwrap_or("—"))
            ));
            s.push_str(&format!("<td>{}</td>", escape_html(&e.result)));
            s.push_str("</tr>");
        }
        s.push_str("</tbody></table>");
        s
    };
    card("Recent logins", "me-login-history", &body)
}

fn render_audit_verify_card() -> String {
    let body = format!(
        r#"<p>Click below to walk the entire audit-log hash chain and report any tampered or out-of-order rows. The check reads every row in <code>action_time</code> order, recomputes its SHA-256 hash, and compares it against the stored value.</p>
<div class="form-actions">
  <button type="button" id="verify-audit" class="btn btn-secondary">
    Run integrity check
  </button>
</div>
{result}
<pre id="verify-result-detail" aria-live="polite" style="margin-top:var(--space-md);font-size:var(--fs-xs);background:var(--c-surface-2);padding:var(--space-md);border-radius:var(--radius-sm);white-space:pre-wrap;overflow-x:auto"></pre>"#,
        result = inline_result("verify-result", ResultTone::Info),
    );
    card("Audit log integrity (admin)", "me-verify", &body)
}

/// Inline script for the action buttons. Kept tiny: read CSRF token from
/// the meta tag (rendered by `layout::wrap`), fetch the relevant endpoint,
/// surface result through the Phase A `inline_result` panels.
fn render_script() -> String {
    r#"<script>
(function () {
  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';
  const withCsrf = (init) => {
    init = init || {};
    init.headers = Object.assign({}, init.headers || {}, csrfToken ? { 'X-CSRF-Token': csrfToken } : {});
    return init;
  };

  const showResult = (panel, tone, msg) => {
    if (!panel) return;
    panel.classList.remove('success', 'error', 'warn', 'info');
    panel.classList.add(tone);
    panel.textContent = msg;
    panel.hidden = false;
  };

  // Revoke other sessions
  const revokeBtn = document.getElementById('revoke-others');
  const revokePanel = document.getElementById('revoke-result');
  revokeBtn?.addEventListener('click', async () => {
    if (!confirm('Log out all other sessions for your account?')) return;
    revokeBtn.disabled = true;
    showResult(revokePanel, 'info', 'Revoking…');
    try {
      const res = await fetch('/api/me/sessions/revoke-others', withCsrf({ method: 'POST' }));
      if (!res.ok) throw new Error(await res.text());
      const body = await res.json();
      showResult(revokePanel, 'success', 'Revoked ' + body.revoked + ' session(s). Reloading…');
      setTimeout(() => location.reload(), 800);
    } catch (e) {
      showResult(revokePanel, 'error', 'Failed: ' + e.message);
      revokeBtn.disabled = false;
    }
  });

  // Audit verify (admin only — button absent for non-admins)
  const verifyBtn = document.getElementById('verify-audit');
  const verifyPanel = document.getElementById('verify-result');
  const verifyDetail = document.getElementById('verify-result-detail');
  verifyBtn?.addEventListener('click', async () => {
    verifyBtn.disabled = true;
    showResult(verifyPanel, 'info', 'Walking the chain…');
    if (verifyDetail) verifyDetail.textContent = '';
    try {
      const res = await fetch('/api/admin/audit/verify');
      if (!res.ok) throw new Error(await res.text());
      const body = await res.json();
      const tampered = (body.tampered_rows || []).length;
      if (tampered === 0) {
        showResult(verifyPanel, 'success', 'Chain intact: ' + body.verified_rows + ' verified, ' + body.legacy_rows + ' legacy.');
      } else {
        showResult(verifyPanel, 'error', 'TAMPERING DETECTED: ' + tampered + ' row(s) failed verification. See details below.');
      }
      if (verifyDetail) verifyDetail.textContent = JSON.stringify(body, null, 2);
    } catch (e) {
      showResult(verifyPanel, 'error', 'Failed: ' + e.message);
    } finally {
      verifyBtn.disabled = false;
    }
  });
})();
</script>
"#
    .to_string()
}

/// Format a unix timestamp (seconds) as an ISO-8601 UTC string.
///
/// Pure helper, unit-tested below. Returns "-" for the placeholder zero
/// value used when a struct hasn't been populated.
pub fn format_unix_ts(ts: i64) -> String {
    if ts == 0 {
        return "-".to_string();
    }
    match chrono::DateTime::<chrono::Utc>::from_timestamp(ts, 0) {
        Some(dt) => dt.format("%Y-%m-%d %H:%M:%S UTC").to_string(),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_unix_ts_returns_dash_for_zero() {
        assert_eq!(format_unix_ts(0), "-");
    }

    #[test]
    fn format_unix_ts_renders_iso_for_known_value() {
        // 2026-05-04 14:03:00 UTC = 1_777_903_380
        // (the function format string is "%Y-%m-%d %H:%M:%S UTC")
        assert_eq!(format_unix_ts(1_777_903_380), "2026-05-04 14:03:00 UTC");
    }

    #[test]
    fn format_unix_ts_handles_epoch() {
        // 1970-01-01T00:00:01 — non-zero, must format normally.
        assert_eq!(format_unix_ts(1), "1970-01-01 00:00:01 UTC");
    }

    #[test]
    fn format_unix_ts_handles_far_future() {
        // 2100-01-01T00:00:00 UTC = 4102444800
        assert_eq!(format_unix_ts(4_102_444_800), "2100-01-01 00:00:00 UTC");
    }
}
