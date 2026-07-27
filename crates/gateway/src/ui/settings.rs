//! Settings page (`/settings`) — admin-only.
//!
//! ## Phase D re-design
//!
//! Settings was previously a read-only display of the user table plus a
//! few system-info rows. Phase D adds an actual user-upsert form so an
//! admin can register a new operator (or promote/demote/deactivate an
//! existing one) without having to leave the UI for the API.
//!
//! User deletion is intentionally not offered — `audit_logs` references
//! `actor_id`, so deleting users would break the audit trail. Instead,
//! deactivation (`is_active = false`) is the supported "remove this
//! person from the system" operation.
//!
//! ## Concept of "danger" actions
//!
//! Promote/demote and deactivate are flagged visually but the actual
//! confirm-then-execute flow stays simple: `POST /api/settings/users`
//! upserts the row, the page reloads, and the audit log records what
//! was changed. We don't need a "diff preview" dialog because the form
//! shows the proposed state before submit and the audit log shows the
//! resulting state after.

use noye_shared::User;

use crate::ui::layout::{card, escape_html, inline_result, ResultTone};

pub fn render(users: &[User]) -> String {
    let mut html = String::new();
    html.push_str(&render_user_management_card(users));
    html.push_str(&render_system_info_card());
    html.push_str(&render_migration_pointer_card());
    html.push_str(&render_script());
    html
}

fn render_user_management_card(users: &[User]) -> String {
    let mut body = String::new();

    body.push_str(&render_users_table(users));
    body.push_str(&render_user_form());
    body.push_str(&format!(
        r#"<div style="margin-top:var(--space-md)">{}</div>"#,
        inline_result("user-form-result", ResultTone::Info),
    ));

    card("User management", "settings-users", &body)
}

fn render_users_table(users: &[User]) -> String {
    if users.is_empty() {
        return r#"<p role="status">No users are registered. The first user is created automatically on first login if their OIDC subject matches an existing record; otherwise they're admitted as a member-only account that you can promote below.</p>"#.to_string();
    }

    let mut html = String::new();
    html.push_str(r#"<table aria-label="Registered users">"#);
    html.push_str(r#"<thead><tr>"#);
    html.push_str(r#"<th scope="col">Name</th>"#);
    html.push_str(r#"<th scope="col">Email</th>"#);
    html.push_str(r#"<th scope="col">Role</th>"#);
    html.push_str(r#"<th scope="col">Status</th>"#);
    html.push_str("</tr></thead><tbody>");
    for user in users {
        let role_badge = match user.role.as_str() {
            "admin" => r#"<span class="badge badge-maint" aria-label="Administrator">admin</span>"#,
            _ => r#"<span class="badge badge-info" aria-label="Member">member</span>"#,
        };
        let status_badge = if user.is_active {
            r#"<span class="badge badge-up" aria-label="Active">active</span>"#
        } else {
            r#"<span class="badge badge-unknown" aria-label="Inactive">inactive</span>"#
        };
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", escape_html(&user.name)));
        html.push_str(&format!("<td>{}</td>", escape_html(&user.email)));
        html.push_str(&format!("<td>{}</td>", role_badge));
        html.push_str(&format!("<td>{}</td>", status_badge));
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
    html
}

fn render_user_form() -> String {
    // Fields: email (key), name, role, is_active. Submitting an existing
    // email overwrites the row (upsert semantics on Core's
    // `users::upsert_user`).
    r##"<details style="margin-top:var(--space-md)">
  <summary><strong>Register or update a user</strong></summary>
  <form id="user-form" style="display:grid;gap:var(--space-md);max-width:36rem;margin-top:var(--space-md)">
    <div class="field">
      <label for="user-email">Email</label>
      <input type="email" id="user-email" name="email" required autocomplete="off">
      <p class="field-help">If a user with this email already exists, their record is updated.</p>
    </div>
    <div class="field">
      <label for="user-name">Display name</label>
      <input type="text" id="user-name" name="name" required>
    </div>
    <div class="field">
      <label for="user-role">Role</label>
      <select id="user-role" name="role" required>
        <option value="member" selected>Member — can view targets they own</option>
        <option value="admin">Administrator — full access</option>
      </select>
      <p class="field-help">Promoting an existing user takes effect on their next login or page reload.</p>
    </div>
    <div class="field">
      <label>
        <input type="checkbox" id="user-active" name="is_active" checked>
        Active (uncheck to deactivate without deleting)
      </label>
      <p class="field-help">Deactivated users can still appear in the audit log (their actor_id is preserved) but cannot log in.</p>
    </div>
    <div class="form-actions">
      <button type="submit" class="btn btn-primary">Save user</button>
    </div>
  </form>
</details>"##.to_string()
}

fn render_system_info_card() -> String {
    let body = r#"<dl class="info-grid">
  <dt>Authentication</dt><dd>Generic OIDC client (Authorization Code + PKCE)</dd>
  <dt>Bot protection</dt><dd>Cloudflare Turnstile (public forms only — currently inactive; see <a href="/admin/migration">configuration migration</a> docs)</dd>
  <dt>Scheduler</dt><dd>Cloudflare Cron Triggers (one minute granularity, target selection internal)</dd>
  <dt>Data storage</dt><dd>D1 (system of record) · KV (cache + sessions) · R2 (long-term archive)</dd>
</dl>"#;
    card("System information", "settings-system", body)
}

fn render_migration_pointer_card() -> String {
    let body = r#"<p>Export this deployment's configuration as JSON, or import a previously-exported configuration. See <a href="/admin/migration">configuration migration</a>.</p>"#;
    card("Migration tools", "settings-migration", body)
}

fn render_script() -> String {
    r#"<script>
(function () {
  const form = document.getElementById('user-form');
  const panel = document.getElementById('user-form-result');
  if (!form || !panel) return;

  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';

  const showResult = (tone, msg) => {
    panel.classList.remove('success', 'error', 'warn', 'info');
    panel.classList.add(tone);
    panel.textContent = msg;
    panel.hidden = false;
  };

  form.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const email = form.email.value.trim();
    const name = form.name.value.trim();
    const role = form.role.value;
    const isActive = form.is_active.checked;
    if (!email || !name) {
      showResult('error', 'Email and display name are required.');
      return;
    }
    showResult('info', 'Saving…');
    try {
      const headers = { 'Content-Type': 'application/json' };
      if (csrfToken) headers['X-CSRF-Token'] = csrfToken;
      const res = await fetch('/api/settings/users', {
        method: 'POST',
        headers,
        body: JSON.stringify({ email, name, role, is_active: isActive }),
      });
      if (!res.ok) throw new Error(await res.text());
      showResult('success', 'Saved. Reloading…');
      setTimeout(() => location.reload(), 700);
    } catch (e) {
      showResult('error', 'Save failed: ' + e.message);
    }
  });
})();
</script>"#.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_user(name: &str, email: &str, role: &str, is_active: bool) -> User {
        User {
            id: "u-1".into(),
            email: email.into(),
            name: name.into(),
            role: role.into(),
            is_active,
            created_at: "2026-04-01T00:00:00Z".into(),
            updated_at: "2026-04-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn render_empty_users_shows_friendly_message_and_form() {
        let html = render(&[]);
        // Empty-state copy explains how the first user gets there.
        assert!(html.contains("No users are registered"));
        // The upsert form is still available so admins can register manually.
        assert!(html.contains(r#"id="user-form""#));
        // Plus the standard system-info / migration sections.
        assert!(html.contains("System information"));
    }

    #[test]
    fn render_user_table_admin_role_uses_maint_badge() {
        let html = render(&[fake_user("Alice", "alice@example.com", "admin", true)]);
        assert!(html.contains(r#"<span class="badge badge-maint" aria-label="Administrator">admin</span>"#));
    }

    #[test]
    fn render_user_table_member_role_uses_info_badge() {
        let html = render(&[fake_user("Bob", "bob@example.com", "member", true)]);
        assert!(html.contains(r#"<span class="badge badge-info" aria-label="Member">member</span>"#));
    }

    #[test]
    fn render_user_table_active_status_uses_up_badge() {
        let html = render(&[fake_user("X", "x@y", "member", true)]);
        assert!(html.contains(r#"<span class="badge badge-up" aria-label="Active">active</span>"#));
    }

    #[test]
    fn render_user_table_inactive_status_uses_unknown_badge() {
        let html = render(&[fake_user("X", "x@y", "member", false)]);
        assert!(html.contains(r#"<span class="badge badge-unknown" aria-label="Inactive">inactive</span>"#));
    }

    #[test]
    fn render_form_includes_required_fields() {
        let html = render(&[]);
        // The form covers the four pieces of ManageUserInput.
        assert!(html.contains(r#"name="email""#));
        assert!(html.contains(r#"name="name""#));
        assert!(html.contains(r#"name="role""#));
        assert!(html.contains(r#"name="is_active""#));
        // Both role values are present.
        assert!(html.contains(r#"value="admin""#));
        assert!(html.contains(r#"value="member""#));
    }

    #[test]
    fn render_form_explains_deactivation_semantics() {
        // Phase D explicitly does NOT offer deletion; the help text
        // explains why deactivation is the supported "remove" path.
        let html = render(&[]);
        let lower = html.to_lowercase();
        assert!(lower.contains("deactivate"));
        assert!(lower.contains("audit log"));
    }

    #[test]
    fn render_includes_inline_result_panel_for_save() {
        // The Phase A inline_result panel is wired so JS can write into it.
        let html = render(&[]);
        assert!(html.contains(r#"id="user-form-result""#));
        assert!(html.contains(r#"aria-live="polite""#));
    }

    #[test]
    fn render_escapes_user_name_and_email() {
        let html = render(&[fake_user("<bad>", "x@<bad>", "admin", true)]);
        assert!(!html.contains("<bad>"));
        assert!(html.contains("&lt;bad&gt;"));
    }

    #[test]
    fn render_system_info_lists_oidc_d1_kv_r2() {
        let html = render(&[]);
        assert!(html.contains("OIDC"));
        assert!(html.contains(">D1 ")); // "D1 (system of record)"
        assert!(html.contains(" KV "));
        assert!(html.contains(" R2 "));
    }

    #[test]
    fn render_migration_card_links_to_migration_page() {
        let html = render(&[]);
        assert!(html.contains(r#"href="/admin/migration""#));
    }
}
