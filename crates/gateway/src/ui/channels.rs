use crate::ui::layout::{escape_html, inline_result, relative_time, ResultTone};
use noye_shared::{AttachedChannel, AttachedTarget, Caller, NotificationChannel};

// ──────────────────────────────────────────────────────────────────
//  Pure helpers (Phase C)
//
//  These power the new "Retry-After" rendering. The JavaScript on the
//  page implements the same logic inline — having a Rust mirror lets
//  us pin the wording in unit tests so the operator-facing message
//  doesn't drift across phases.
// ──────────────────────────────────────────────────────────────────

/// Parse an HTTP `Retry-After` header value as a non-negative integer
/// seconds count.
///
/// The HTTP spec also allows an HTTP-date form, but our rate limiter
/// only emits the seconds form, so we accept just that. Returns
/// `None` for empty / non-numeric / negative input.
///
/// Currently exercised only in unit tests (the JavaScript on the page
/// reimplements the same logic so the page works without server round-
/// trips). Kept as a `pub` Rust mirror so the wording can be pinned by
/// the test, preventing UI / Rust drift.
#[allow(dead_code)]
pub fn parse_retry_after(header: &str) -> Option<i64> {
    let trimmed = header.trim();
    if trimmed.is_empty() {
        return None;
    }
    let n: i64 = trimmed.parse().ok()?;
    if n < 0 {
        None
    } else {
        Some(n)
    }
}

/// Build a human-readable "wait this long" hint from a Retry-After
/// seconds value. The output uses the largest meaningful unit so a
/// 90-second hint reads "about 1.5 minutes" rather than "90 seconds".
///
/// Returned strings are deliberately plain — no emoji, no exclamation
/// marks. Operators see this in a stress moment; calm phrasing wins.
///
/// As with `parse_retry_after`, the JavaScript on the page mirrors
/// this; the Rust copy exists to keep the wording pinned by unit
/// tests so the two implementations don't drift.
#[allow(dead_code)]
pub fn format_retry_after_hint(seconds: i64) -> String {
    let s = seconds.max(0);
    if s < 60 {
        format!("Try again in about {} second{}.", s, if s == 1 { "" } else { "s" })
    } else if s < 3600 {
        let mins = s as f64 / 60.0;
        // Round to one decimal but trim ".0" for whole minutes.
        let rendered = if (mins - mins.round()).abs() < 0.05 {
            format!("{:.0}", mins.round())
        } else {
            format!("{:.1}", mins)
        };
        format!("Try again in about {} minute{}.", rendered, if rendered == "1" { "" } else { "s" })
    } else {
        let hours = s as f64 / 3600.0;
        let rendered = if (hours - hours.round()).abs() < 0.05 {
            format!("{:.0}", hours.round())
        } else {
            format!("{:.1}", hours)
        };
        format!("Try again in about {} hour{}.", rendered, if rendered == "1" { "" } else { "s" })
    }
}

/// Render the list page at `/channels`.
pub fn render_list(channels: &[NotificationChannel], caller: &Caller) -> String {
    let mut html = String::new();

    if caller.is_admin() {
        html.push_str(&render_create_form());
    }

    html.push_str(r#"<div class="card">"#);

    if channels.is_empty() {
        html.push_str("<p>No notification channels are configured.</p>");
    } else {
        html.push_str(r#"<table aria-label="notification channels list">"#);
        html.push_str(
            "<thead><tr>\
                <th scope=\"col\">Name</th>\
                <th scope=\"col\">Type</th>\
                <th scope=\"col\">Endpoint</th>\
                <th scope=\"col\">Enabled</th>\
                <th scope=\"col\">Created</th>\
                <th scope=\"col\">Actions</th>\
             </tr></thead><tbody>",
        );

        for ch in channels {
            html.push_str("<tr>");
            html.push_str(&format!(
                r#"<td><a href="/channels/{id}">{name}</a></td>"#,
                id = escape_html(&ch.id),
                name = escape_html(&ch.name),
            ));
            html.push_str(&format!(
                r#"<td><span class="badge" data-kind="type">{}</span></td>"#,
                escape_html(&ch.channel_type)
            ));
            html.push_str(&format!(
                "<td><code>{}</code></td>",
                escape_html(&mask_endpoint(&ch.endpoint, &ch.channel_type))
            ));
            html.push_str(&format!(
                "<td>{}</td>",
                if ch.is_enabled { "Yes" } else { "No" }
            ));
            html.push_str(&format!("<td>{}</td>", relative_time(&ch.created_at)));

            if caller.is_admin() {
                html.push_str(&format!(
                    r#"<td>
                        <button type="button"
                                class="action-test"
                                data-channel-id="{id}"
                                data-channel-enabled="{enabled}"
                                aria-label="Send test notification via {name}"
                                {disabled_attr}>
                          Send test
                        </button>
                        <button type="button"
                                class="action-toggle"
                                data-channel-id="{id}"
                                data-current-enabled="{enabled}"
                                aria-label="Toggle enabled state of {name}">
                          {label}
                        </button>
                        <button type="button"
                                class="action-delete"
                                data-channel-id="{id}"
                                aria-label="Delete channel {name}">
                          Delete
                        </button>
                       </td>"#,
                    id = escape_html(&ch.id),
                    name = escape_html(&ch.name),
                    enabled = ch.is_enabled,
                    label = if ch.is_enabled { "Disable" } else { "Enable" },
                    disabled_attr = if ch.is_enabled { "" } else { "disabled aria-disabled=\"true\" title=\"Enable the channel before testing\"" },
                ));
            } else {
                html.push_str("<td>—</td>");
            }
            html.push_str("</tr>");
        }
        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");

    if caller.is_admin() {
        // Single inline result region for create/toggle/delete/test
        // actions on the channels list page. The script writes into it.
        html.push_str(&format!(
            r#"<div class="card" aria-labelledby="ch-list-result-heading">
  <h3 id="ch-list-result-heading" class="sr-only">Action result</h3>
  {result}
</div>"#,
            result = inline_result("channel-list-result", ResultTone::Info),
        ));
        html.push_str(&render_management_script());
    }

    html
}

/// Inline section for the target detail page that lists attached channels and
/// (for admins) lets them attach more.
pub fn render_target_attachments(
    target_id: &str,
    attached: &[AttachedChannel],
    available: &[NotificationChannel],
    caller: &Caller,
) -> String {
    let mut html = String::new();
    html.push_str(r#"<section class="card" aria-labelledby="channels-heading">"#);
    html.push_str(r#"<h3 id="channels-heading">Notification channels</h3>"#);

    if attached.is_empty() {
        html.push_str("<p>No channels are attached to this target.</p>");
    } else {
        html.push_str(r#"<table aria-label="attached channels">"#);
        html.push_str(
            "<thead><tr>\
                <th scope=\"col\">Name</th>\
                <th scope=\"col\">Type</th>\
                <th scope=\"col\">On down</th>\
                <th scope=\"col\">On up</th>\
                <th scope=\"col\">Active</th>\
                <th scope=\"col\">Actions</th>\
             </tr></thead><tbody>",
        );
        for ac in attached {
            html.push_str("<tr>");
            html.push_str(&format!("<td>{}</td>", escape_html(&ac.channel_name)));
            html.push_str(&format!(
                r#"<td><span class="badge" data-kind="type">{}</span></td>"#,
                escape_html(&ac.channel_type)
            ));
            html.push_str(&format!(
                "<td>{}</td>",
                if ac.on_down { "Yes" } else { "No" }
            ));
            html.push_str(&format!("<td>{}</td>", if ac.on_up { "Yes" } else { "No" }));
            html.push_str(&format!(
                "<td>{}</td>",
                if ac.is_enabled { "Yes" } else { "No" }
            ));
            if caller.is_admin() {
                html.push_str(&format!(
                    r#"<td><button type="button"
                                  class="action-detach"
                                  data-target-id="{tid}"
                                  data-channel-id="{cid}"
                                  aria-label="Detach channel {name}">
                              Detach
                            </button></td>"#,
                    tid = escape_html(target_id),
                    cid = escape_html(&ac.channel_id),
                    name = escape_html(&ac.channel_name),
                ));
            } else {
                html.push_str("<td>—</td>");
            }
            html.push_str("</tr>");
        }
        html.push_str("</tbody></table>");
    }

    if caller.is_admin() {
        html.push_str(&render_attach_form(target_id, attached, available));
        // Inline result region for attach/detach actions.
        html.push_str(&format!(
            r#"<div style="margin-top:var(--space-md)">{result}</div>"#,
            result = inline_result("channel-attach-result", ResultTone::Info),
        ));
        html.push_str(&render_attach_script());
    }

    html.push_str("</section>");
    html
}

/// Channel detail / edit page at `/channels/:id`.
///
/// Composition:
/// 1. Editable form for `name`, `endpoint`, and `is_enabled`. The
///    `channel_type` is intentionally read-only — changing the type would
///    invalidate the endpoint format, so the operator should delete and
///    recreate instead.
/// 2. Action row: Send test, Delete.
/// 3. "Targets attached to this channel" section so the operator can see the
///    blast radius before mutating.
pub fn render_detail(channel: &NotificationChannel, attached: &[AttachedTarget], caller: &Caller) -> String {
    let mut html = String::new();

    // ── Identity card ──
    html.push_str(r#"<section class="card" aria-labelledby="ch-identity">"#);
    html.push_str(r#"<h3 id="ch-identity">Identity</h3>"#);
    html.push_str("<dl>");
    html.push_str(&format!("<dt>ID</dt><dd><code>{}</code></dd>", escape_html(&channel.id)));
    html.push_str(&format!(
        "<dt>Type</dt><dd><span class=\"badge\" data-kind=\"type\">{}</span></dd>",
        escape_html(&channel.channel_type)
    ));
    html.push_str(&format!("<dt>Created</dt><dd>{}</dd>", relative_time(&channel.created_at)));
    html.push_str(&format!("<dt>Owner</dt><dd><code>{}</code></dd>", escape_html(&channel.owner_id)));
    html.push_str("</dl>");
    html.push_str("</section>");

    // ── Edit form (admin only) ──
    if caller.is_admin() {
        html.push_str(&render_edit_form(channel));
        html.push_str(&render_actions(channel));
        // Single inline result region for all admin actions on this
        // page (save / test / delete). The script writes into it and
        // toggles the tone class instead of popping alerts.
        html.push_str(&format!(
            r#"<div class="card" aria-labelledby="ch-result-heading">
  <h3 id="ch-result-heading" class="sr-only">Action result</h3>
  {result}
</div>"#,
            result = inline_result("channel-action-result", ResultTone::Info),
        ));
        html.push_str(&render_detail_script(&channel.id));
    } else {
        html.push_str(r#"<section class="card"><p>You can view this channel but you do not have permission to edit it.</p></section>"#);
    }

    // ── Reverse-lookup: targets that use this channel ──
    html.push_str(&render_attached_targets(attached));

    html
}

fn render_edit_form(channel: &NotificationChannel) -> String {
    let endpoint_hint = match channel.channel_type.as_str() {
        "webhook" | "slack" => "Must start with https://",
        "email" => "Must contain a single '@' and a dotted domain",
        _ => "",
    };
    format!(
        r#"<section class="card" aria-labelledby="ch-edit-heading">
  <h3 id="ch-edit-heading">Edit</h3>
  <form id="channel-edit"
        data-channel-id="{id}"
        style="display:grid;gap:var(--space-sm);max-width:36rem">
    <label>Name
      <input type="text" name="name" value="{name}" required>
    </label>
    <label>Endpoint
      <input type="text" name="endpoint" value="{endpoint}" required
             aria-describedby="ch-endpoint-hint">
      <small id="ch-endpoint-hint">{hint}</small>
    </label>
    <label>
      <input type="checkbox" name="is_enabled" {checked}>
      Enabled (disable to silence without losing the configuration)
    </label>
    <div>
      <button type="submit">Save changes</button>
      <a href="/channels" class="button-link" style="margin-left:var(--space-sm)">Cancel</a>
    </div>
    <output id="channel-edit-result" role="status" aria-live="polite"></output>
  </form>
</section>"#,
        id = escape_html(&channel.id),
        name = escape_html(&channel.name),
        endpoint = escape_html(&channel.endpoint),
        hint = endpoint_hint,
        checked = if channel.is_enabled { "checked" } else { "" },
    )
}

fn render_actions(channel: &NotificationChannel) -> String {
    let test_disabled = if channel.is_enabled {
        ""
    } else {
        "disabled aria-disabled=\"true\" title=\"Enable the channel before testing\""
    };
    format!(
        r#"<section class="card" aria-labelledby="ch-actions-heading">
  <h3 id="ch-actions-heading">Actions</h3>
  <div style="display:flex;gap:var(--space-sm);flex-wrap:wrap">
    <button type="button"
            class="action-test"
            data-channel-id="{id}"
            aria-label="Send test notification via {name}"
            {disabled_attr}>
      Send test
    </button>
    <button type="button"
            class="action-delete"
            data-channel-id="{id}"
            aria-label="Delete channel {name}">
      Delete
    </button>
  </div>
</section>"#,
        id = escape_html(&channel.id),
        name = escape_html(&channel.name),
        disabled_attr = test_disabled,
    )
}

fn render_attached_targets(attached: &[AttachedTarget]) -> String {
    let mut html = String::new();
    html.push_str(r#"<section class="card" aria-labelledby="ch-targets-heading">"#);
    html.push_str(r#"<h3 id="ch-targets-heading">Targets using this channel</h3>"#);

    if attached.is_empty() {
        html.push_str("<p>This channel is not attached to any target. Editing or deleting it has no immediate effect on monitoring.</p>");
    } else {
        html.push_str(&format!(
            "<p><strong>{}</strong> {} attached. Changes here will affect notifications for {}.</p>",
            attached.len(),
            if attached.len() == 1 { "target is" } else { "targets are" },
            if attached.len() == 1 { "this target" } else { "these targets" },
        ));
        html.push_str(r#"<table aria-label="targets attached to this channel">"#);
        html.push_str(
            "<thead><tr>\
                <th scope=\"col\">Name</th>\
                <th scope=\"col\">Type</th>\
                <th scope=\"col\">Host</th>\
                <th scope=\"col\">On down</th>\
                <th scope=\"col\">On up</th>\
             </tr></thead><tbody>",
        );
        for at in attached {
            html.push_str("<tr>");
            html.push_str(&format!(
                r#"<td><a href="/targets/{id}">{name}</a></td>"#,
                id = escape_html(&at.target_id),
                name = escape_html(&at.target_name),
            ));
            html.push_str(&format!(
                r#"<td><span class="badge" data-kind="type">{}</span></td>"#,
                escape_html(&at.target_type),
            ));
            html.push_str(&format!("<td>{}</td>", escape_html(&at.target_host)));
            html.push_str(&format!("<td>{}</td>", if at.on_down { "Yes" } else { "No" }));
            html.push_str(&format!("<td>{}</td>", if at.on_up { "Yes" } else { "No" }));
            html.push_str("</tr>");
        }
        html.push_str("</tbody></table>");
    }
    html.push_str("</section>");
    html
}

fn render_detail_script(channel_id: &str) -> String {
    format!(
        r#"<script>
(function () {{
  const cid = {cid_json};
  const panel = document.getElementById('channel-action-result');
  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';

  // Toggle the panel's tone class and message. Tones map to the four
  // inline-result CSS classes from style.rs section 10.
  const showResult = (tone, msg) => {{
    if (!panel) return;
    panel.classList.remove('success', 'error', 'warn', 'info');
    panel.classList.add(tone);
    panel.textContent = msg;
    panel.hidden = false;
  }};
  const clearResult = () => {{ if (panel) panel.hidden = true; }};

  // Same Retry-After hint as the Rust mirror in `format_retry_after_hint`.
  // Kept here so the page works without server round-trips.
  const formatRetryHint = (raw) => {{
    if (raw === null || raw === undefined || raw === '') return '';
    const n = parseInt(raw, 10);
    if (!Number.isFinite(n) || n < 0) return '';
    if (n < 60) {{
      return 'Try again in about ' + n + ' second' + (n === 1 ? '' : 's') + '.';
    }}
    if (n < 3600) {{
      const m = n / 60;
      const r = Math.abs(m - Math.round(m)) < 0.05 ? Math.round(m).toString() : m.toFixed(1);
      return 'Try again in about ' + r + ' minute' + (r === '1' ? '' : 's') + '.';
    }}
    const h = n / 3600;
    const r = Math.abs(h - Math.round(h)) < 0.05 ? Math.round(h).toString() : h.toFixed(1);
    return 'Try again in about ' + r + ' hour' + (r === '1' ? '' : 's') + '.';
  }};

  const withCsrf = (init) => {{
    init = init || {{}};
    init.headers = Object.assign({{}}, init.headers || {{}}, csrfToken ? {{ 'X-CSRF-Token': csrfToken }} : {{}});
    return init;
  }};

  document.getElementById('channel-edit')?.addEventListener('submit', async (ev) => {{
    ev.preventDefault();
    const f = ev.currentTarget;
    const body = {{
      name: f.name.value,
      endpoint: f.endpoint.value,
      is_enabled: f.is_enabled.checked,
    }};
    showResult('info', 'Saving…');
    try {{
      const res = await fetch('/api/channels/' + encodeURIComponent(cid), withCsrf({{
        method: 'PUT',
        headers: {{ 'Content-Type': 'application/json' }},
        body: JSON.stringify(body),
      }}));
      if (!res.ok) throw new Error(await res.text());
      showResult('success', 'Saved.');
      setTimeout(() => location.reload(), 600);
    }} catch (e) {{
      showResult('error', 'Save failed: ' + e.message);
    }}
  }});

  document.querySelectorAll('.action-test').forEach((btn) => {{
    btn.addEventListener('click', async () => {{
      if (btn.disabled) return;
      const original = btn.textContent;
      btn.disabled = true;
      btn.textContent = 'Sending…';
      clearResult();
      try {{
        const res = await fetch('/api/channels/' + encodeURIComponent(cid) + '/test', withCsrf({{ method: 'POST' }}));
        if (res.status === 429) {{
          // Rate-limit hit. Translate the Retry-After seconds into a
          // human hint instead of dumping the raw value.
          const wait = res.headers.get('Retry-After') || '';
          const hint = formatRetryHint(wait);
          const text = await res.text();
          showResult('warn', text + (hint ? ' ' + hint : ''));
          return;
        }}
        if (!res.ok) throw new Error(await res.text());
        showResult('success', 'Test notification dispatched. Verify on the channel side.');
      }} catch (e) {{
        showResult('error', 'Test send failed: ' + e.message);
      }} finally {{
        btn.disabled = false;
        btn.textContent = original;
      }}
    }});
  }});

  document.querySelectorAll('.action-delete').forEach((btn) => {{
    btn.addEventListener('click', async () => {{
      if (!confirm('Delete this channel and all of its target attachments?')) return;
      try {{
        const res = await fetch('/api/channels/' + encodeURIComponent(cid), withCsrf({{ method: 'DELETE' }}));
        if (!res.ok) throw new Error(await res.text());
        location.href = '/channels';
      }} catch (e) {{ showResult('error', 'Delete failed: ' + e.message); }}
    }});
  }});
}})();
</script>"#,
        cid_json = serde_json::to_string(channel_id).unwrap_or_else(|_| "\"\"".to_string()),
    )
}

// ── Helpers ──

fn render_create_form() -> String {
    r#"<div class="card" style="margin-bottom:var(--space-lg)">
  <details>
    <summary><strong>+ Create notification channel</strong></summary>
    <form id="channel-create" style="margin-top:var(--space-md);display:grid;gap:var(--space-sm);max-width:36rem">
      <label>Name <input type="text" name="name" required></label>
      <label>Type
        <select name="channel_type" required>
          <option value="webhook">webhook (HTTPS POST)</option>
          <option value="slack">slack (incoming webhook)</option>
          <option value="email">email</option>
        </select>
      </label>
      <label>Endpoint
        <input type="text" name="endpoint" placeholder="https://… or you@example.com" required>
      </label>
      <button type="submit">Create</button>
      <output id="channel-create-result" role="status" aria-live="polite"></output>
    </form>
  </details>
</div>"#.to_string()
}

fn render_management_script() -> String {
    r#"<script>
(function () {
  const panel = document.getElementById('channel-list-result');
  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';

  const showResult = (tone, msg) => {
    if (!panel) return;
    panel.classList.remove('success', 'error', 'warn', 'info');
    panel.classList.add(tone);
    panel.textContent = msg;
    panel.hidden = false;
  };
  const clearResult = () => { if (panel) panel.hidden = true; };

  const formatRetryHint = (raw) => {
    if (raw === null || raw === undefined || raw === '') return '';
    const n = parseInt(raw, 10);
    if (!Number.isFinite(n) || n < 0) return '';
    if (n < 60) return 'Try again in about ' + n + ' second' + (n === 1 ? '' : 's') + '.';
    if (n < 3600) {
      const m = n / 60;
      const r = Math.abs(m - Math.round(m)) < 0.05 ? Math.round(m).toString() : m.toFixed(1);
      return 'Try again in about ' + r + ' minute' + (r === '1' ? '' : 's') + '.';
    }
    const h = n / 3600;
    const r = Math.abs(h - Math.round(h)) < 0.05 ? Math.round(h).toString() : h.toFixed(1);
    return 'Try again in about ' + r + ' hour' + (r === '1' ? '' : 's') + '.';
  };

  const withCsrf = (init) => {
    init = init || {};
    init.headers = Object.assign({}, init.headers || {}, csrfToken ? { 'X-CSRF-Token': csrfToken } : {});
    return init;
  };

  document.getElementById('channel-create')?.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const f = ev.currentTarget;
    const body = {
      name: f.name.value,
      channel_type: f.channel_type.value,
      endpoint: f.endpoint.value,
    };
    showResult('info', 'Creating channel…');
    try {
      const res = await fetch('/api/channels', withCsrf({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }));
      if (!res.ok) throw new Error(await res.text());
      location.reload();
    } catch (e) { showResult('error', 'Create failed: ' + e.message); }
  });

  document.querySelectorAll('.action-toggle').forEach((btn) => {
    btn.addEventListener('click', async () => {
      const id = btn.dataset.channelId;
      const next = btn.dataset.currentEnabled !== 'true';
      try {
        const res = await fetch('/api/channels/' + encodeURIComponent(id), withCsrf({
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ is_enabled: next }),
        }));
        if (!res.ok) throw new Error(await res.text());
        location.reload();
      } catch (e) { showResult('error', 'Toggle failed: ' + e.message); }
    });
  });

  document.querySelectorAll('.action-delete').forEach((btn) => {
    btn.addEventListener('click', async () => {
      if (!confirm('Delete this channel and all of its target attachments?')) return;
      const id = btn.dataset.channelId;
      try {
        const res = await fetch('/api/channels/' + encodeURIComponent(id), withCsrf({ method: 'DELETE' }));
        if (!res.ok) throw new Error(await res.text());
        location.reload();
      } catch (e) { showResult('error', 'Delete failed: ' + e.message); }
    });
  });

  document.querySelectorAll('.action-test').forEach((btn) => {
    btn.addEventListener('click', async () => {
      if (btn.disabled) return;
      const id = btn.dataset.channelId;
      const original = btn.textContent;
      btn.disabled = true;
      btn.textContent = 'Sending…';
      clearResult();
      try {
        const res = await fetch('/api/channels/' + encodeURIComponent(id) + '/test', withCsrf({ method: 'POST' }));
        if (res.status === 429) {
          const wait = res.headers.get('Retry-After') || '';
          const hint = formatRetryHint(wait);
          const text = await res.text();
          showResult('warn', text + (hint ? ' ' + hint : ''));
          return;
        }
        if (!res.ok) throw new Error(await res.text());
        showResult('success', 'Test notification dispatched. Verify on the channel side.');
      } catch (e) {
        showResult('error', 'Test send failed: ' + e.message);
      } finally {
        btn.disabled = false;
        btn.textContent = original;
      }
    });
  });
})();
</script>"#.to_string()
}

fn render_attach_form(
    target_id: &str,
    attached: &[AttachedChannel],
    available: &[NotificationChannel],
) -> String {
    // Filter out channels already attached to avoid duplicate work and
    // confusing UX. The backend is idempotent but the form should not offer
    // a no-op option.
    let attached_ids: std::collections::HashSet<&str> =
        attached.iter().map(|a| a.channel_id.as_str()).collect();

    let options: String = available
        .iter()
        .filter(|c| !attached_ids.contains(c.id.as_str()))
        .map(|c| {
            format!(
                r#"<option value="{id}">{name} ({ctype})</option>"#,
                id = escape_html(&c.id),
                name = escape_html(&c.name),
                ctype = escape_html(&c.channel_type),
            )
        })
        .collect();

    if options.is_empty() {
        return r#"<p style="margin-top:var(--space-md)"><em>All available channels are already attached to this target.</em></p>"#.to_string();
    }

    format!(
        r#"<details style="margin-top:var(--space-md)">
  <summary><strong>+ Attach channel</strong></summary>
  <form id="channel-attach" data-target-id="{tid}" style="margin-top:var(--space-md);display:grid;gap:var(--space-sm);max-width:30rem">
    <label>Channel
      <select name="channel_id" required>
        <option value="" disabled selected>Choose a channel…</option>
        {options}
      </select>
    </label>
    <label><input type="checkbox" name="on_down" checked> Notify on outage (down)</label>
    <label><input type="checkbox" name="on_up" checked> Notify on recovery (up)</label>
    <button type="submit">Attach</button>
  </form>
</details>"#,
        tid = escape_html(target_id),
        options = options
    )
}

fn render_attach_script() -> String {
    r#"<script>
(function () {
  const panel = document.getElementById('channel-attach-result');
  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';

  const showResult = (tone, msg) => {
    if (!panel) return;
    panel.classList.remove('success', 'error', 'warn', 'info');
    panel.classList.add(tone);
    panel.textContent = msg;
    panel.hidden = false;
  };

  const withCsrf = (init) => {
    init = init || {};
    init.headers = Object.assign({}, init.headers || {}, csrfToken ? { 'X-CSRF-Token': csrfToken } : {});
    return init;
  };

  document.getElementById('channel-attach')?.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const f = ev.currentTarget;
    const tid = f.dataset.targetId;
    const body = {
      channel_id: f.channel_id.value,
      on_down: f.on_down.checked,
      on_up: f.on_up.checked,
    };
    showResult('info', 'Attaching channel…');
    try {
      const res = await fetch('/api/targets/' + encodeURIComponent(tid) + '/channels', withCsrf({
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
      }));
      if (!res.ok) throw new Error(await res.text());
      location.reload();
    } catch (e) { showResult('error', 'Attach failed: ' + e.message); }
  });

  document.querySelectorAll('.action-detach').forEach((btn) => {
    btn.addEventListener('click', async () => {
      if (!confirm('Detach this channel from the target?')) return;
      const tid = btn.dataset.targetId;
      const cid = btn.dataset.channelId;
      try {
        const res = await fetch(
          '/api/targets/' + encodeURIComponent(tid) + '/channels/' + encodeURIComponent(cid),
          withCsrf({ method: 'DELETE' })
        );
        if (!res.ok) throw new Error(await res.text());
        location.reload();
      } catch (e) { showResult('error', 'Detach failed: ' + e.message); }
    });
  });
})();
</script>"#.to_string()
}

/// Mask sensitive parts of an endpoint for display.
///
/// - For email: keep the local-part prefix and the domain, hide the middle
///   (`alice@example.com` -> `a***@example.com`).
/// - For URLs: hide path and query (`https://hooks.slack.com/services/T/B/X` ->
///   `https://hooks.slack.com/...`).
///
/// This is best-effort obfuscation for shoulder-surfing. The full value is
/// always recoverable via the API for admins.
pub fn mask_endpoint(endpoint: &str, channel_type: &str) -> String {
    match channel_type {
        "email" => match endpoint.split_once('@') {
            Some((local, domain)) if !local.is_empty() => {
                let head: String = local.chars().take(1).collect();
                format!("{}***@{}", head, domain)
            }
            _ => endpoint.to_string(),
        },
        "webhook" | "slack" => {
            // Find third "/" (after "https://") and truncate.
            if let Some(after_scheme) = endpoint.strip_prefix("https://") {
                if let Some(slash) = after_scheme.find('/') {
                    return format!("https://{}/…", &after_scheme[..slash]);
                }
            }
            endpoint.to_string()
        }
        _ => endpoint.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_email_keeps_one_char_and_domain() {
        assert_eq!(mask_endpoint("alice@example.com", "email"), "a***@example.com");
        assert_eq!(
            mask_endpoint("ops@sub.example.co.jp", "email"),
            "o***@sub.example.co.jp"
        );
    }

    #[test]
    fn mask_email_returns_input_for_malformed_address() {
        // The validator should reject these before they reach the renderer,
        // but the masker stays defensive.
        assert_eq!(mask_endpoint("not-an-email", "email"), "not-an-email");
        assert_eq!(mask_endpoint("@nohost.com", "email"), "@nohost.com");
    }

    #[test]
    fn mask_webhook_truncates_to_host() {
        assert_eq!(
            mask_endpoint("https://hooks.example.com/services/T/B/X", "webhook"),
            "https://hooks.example.com/…"
        );
        assert_eq!(
            mask_endpoint("https://hooks.slack.com/services/T/B/X", "slack"),
            "https://hooks.slack.com/…"
        );
    }

    #[test]
    fn mask_webhook_with_no_path_returns_input() {
        assert_eq!(mask_endpoint("https://example.com", "webhook"), "https://example.com");
    }

    #[test]
    fn mask_unknown_type_returns_input_unchanged() {
        assert_eq!(mask_endpoint("anything", "carrier-pigeon"), "anything");
    }

    // ── parse_retry_after ──

    #[test]
    fn parse_retry_after_accepts_seconds() {
        assert_eq!(parse_retry_after("30"), Some(30));
        assert_eq!(parse_retry_after("0"), Some(0));
        assert_eq!(parse_retry_after("3600"), Some(3600));
    }

    #[test]
    fn parse_retry_after_trims_whitespace() {
        assert_eq!(parse_retry_after("  45  "), Some(45));
    }

    #[test]
    fn parse_retry_after_rejects_negative_and_garbage() {
        assert_eq!(parse_retry_after(""), None);
        assert_eq!(parse_retry_after("-1"), None);
        assert_eq!(parse_retry_after("abc"), None);
        // The HTTP-date form is part of the spec but our rate limiter
        // never emits it, so we return None and the caller falls back
        // to "no specific hint" — better than misinterpreting.
        assert_eq!(parse_retry_after("Wed, 21 Oct 2026 07:28:00 GMT"), None);
    }

    // ── format_retry_after_hint ──

    #[test]
    fn retry_hint_seconds_under_a_minute() {
        assert_eq!(format_retry_after_hint(0), "Try again in about 0 seconds.");
        assert_eq!(format_retry_after_hint(1), "Try again in about 1 second.");
        assert_eq!(format_retry_after_hint(30), "Try again in about 30 seconds.");
        assert_eq!(format_retry_after_hint(59), "Try again in about 59 seconds.");
    }

    #[test]
    fn retry_hint_minutes_for_one_to_sixty_minutes() {
        // A whole minute boundary renders without decimals.
        assert_eq!(format_retry_after_hint(60), "Try again in about 1 minute.");
        assert_eq!(format_retry_after_hint(120), "Try again in about 2 minutes.");
        // A non-whole value rounds to one decimal.
        assert_eq!(format_retry_after_hint(90), "Try again in about 1.5 minutes.");
    }

    #[test]
    fn retry_hint_hours_above_one_hour() {
        assert_eq!(format_retry_after_hint(3600), "Try again in about 1 hour.");
        assert_eq!(format_retry_after_hint(7200), "Try again in about 2 hours.");
        assert_eq!(format_retry_after_hint(5400), "Try again in about 1.5 hours.");
    }

    #[test]
    fn retry_hint_clamps_negative_to_zero() {
        // Defense-in-depth: parse_retry_after rejects negatives, but
        // if a caller bypasses the parser we still produce a sensible
        // string instead of "Try again in about -30 seconds."
        assert_eq!(format_retry_after_hint(-30), "Try again in about 0 seconds.");
    }
}
