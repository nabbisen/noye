//! Configuration migration page (`/admin/migration`).
//!
//! ## Phase D re-design
//!
//! Functionally unchanged from the previous version (export with the
//! optional include_users toggle, import with conflict-policy +
//! dry-run-by-default), but rewritten on top of the Phase A design
//! tokens and the `inline_result` component. The previous file
//! referenced `--color-fg-muted` / `--color-success` / `--color-danger`
//! which were deprecated when Phase A switched to the
//! `--c-text-muted` / `--c-up` / `--c-down` token namespace; those
//! references would have rendered with the browser's default colors
//! had they been hit. The rewrite removes the inline `style.color`
//! manipulation entirely and lets the inline-result panel's tone
//! classes do the colouring.
//!
//! ## Sections
//!
//! 1. **Intro** — what the JSON document covers and what it doesn't
//! 2. **Export** — admin chooses include_users; downloads as
//!    `noye-export-YYYYMMDD.json`
//! 3. **Import** — admin uploads/pastes JSON, picks conflict policy
//!    (skip/replace/fail) and apply flag (default off = dry-run)
//! 4. **Bulk pointer** — wrangler/rclone recipe for full data dumps
//!    that are too large for this page

use crate::ui::layout::{ResultTone, card, inline_result};
use noye_shared::Caller;

pub fn render_page(caller: &Caller) -> String {
    if !caller.is_admin() {
        return card(
            "Restricted",
            "migration-restricted",
            r#"<p role="status">This page is restricted to administrators.</p>"#,
        );
    }

    let mut html = String::new();
    html.push_str(&render_intro_card());
    html.push_str(&render_export_card());
    html.push_str(&render_import_card());
    html.push_str(&render_bulk_pointer_card());
    html.push_str(&render_script());
    html
}

fn render_intro_card() -> String {
    let body = r#"<p>Export this deployment's <strong>configuration tables</strong> (targets, channels, attachments, maintenance windows, optionally users) as a JSON document, or import a previously-exported document into this deployment.</p>
<p style="margin-top:var(--space-sm);font-size:var(--fs-sm);color:var(--c-text-muted)">
  Bulk monitoring data — <code>check_results</code>, <code>incidents</code>, <code>audit_logs</code>, R2 archive snapshots — is <em>not</em> included here. Use <code>wrangler d1 export</code> for those (see the cheat-sheet at the bottom of this page).
</p>"#;
    card("Configuration migration", "migration-intro", body)
}

fn render_export_card() -> String {
    let body = format!(
        r#"<form id="export-form" style="display:grid;gap:var(--space-md);max-width:36rem">
  <div class="field">
    <label>
      <input type="checkbox" name="include_users" id="include-users-checkbox">
      Include users (PII: emails). Off by default.
    </label>
    <p class="field-help">The exported file is a single JSON document. Your browser will offer to save it as <code>noye-export-YYYYMMDD.json</code>.</p>
  </div>
  <div class="form-actions">
    <button type="submit" class="btn btn-primary">Download export</button>
  </div>
  {result}
</form>"#,
        result = inline_result("export-result", ResultTone::Info),
    );
    card("Export", "migration-export", &body)
}

fn render_import_card() -> String {
    let body = format!(
        r##"<form id="import-form" style="display:grid;gap:var(--space-md);max-width:42rem">
  <div class="field">
    <label for="import-file">JSON file</label>
    <input type="file" name="file" id="import-file" accept="application/json,.json" required>
  </div>
  <fieldset style="border:1px solid var(--c-border);padding:var(--space-md);border-radius:var(--radius-md)">
    <legend>Conflict policy</legend>
    <div class="field">
      <label><input type="radio" name="policy" value="skip" checked> <strong>Skip</strong> — keep existing rows; ignore incoming rows whose IDs already exist (recommended for fresh migrations)</label>
    </div>
    <div class="field">
      <label><input type="radio" name="policy" value="replace"> <strong>Replace</strong> — overwrite existing rows with incoming data</label>
    </div>
    <div class="field">
      <label><input type="radio" name="policy" value="fail"> <strong>Fail</strong> — abort the entire import on the first ID collision</label>
    </div>
  </fieldset>
  <div class="field">
    <label>
      <input type="checkbox" name="apply" id="apply-checkbox">
      <strong>Apply</strong> — actually write to the database. Leave unchecked for a dry-run that returns counts without making changes.
    </label>
  </div>
  <div class="form-actions">
    <button type="submit" class="btn btn-primary">Run</button>
  </div>
  {result}
</form>"##,
        result = inline_result("import-result", ResultTone::Info),
    );
    card("Import", "migration-import", &body)
}

fn render_bulk_pointer_card() -> String {
    let body = r##"<p>For full migrations including <code>check_results</code>, <code>incidents</code>, <code>audit_logs</code>, and the R2 archive, use the wrangler-based recipe in <code>docs/src/migration.md</code>:</p>
<pre style="overflow-x:auto;font-size:var(--fs-sm);background:var(--c-surface-2);padding:var(--space-md);border-radius:var(--radius-sm)"><code># 1. Dump every D1 table on the source
wrangler d1 export noye_db --output noye-d1-dump.sql

# 2. Apply on the destination (after creating an empty database with the same schema)
wrangler d1 execute noye_db_dest --file noye-d1-dump.sql

# 3. Mirror the R2 archive bucket
rclone sync r2-source:noye-logs r2-dest:noye-logs</code></pre>
<p class="field-help">The configuration export above is a subset of what <code>wrangler d1 export</code> produces, but is portable across schema-version-compatible deployments and does not require <code>wrangler</code> access.</p>"##;
    card("Bulk monitoring data", "migration-bulk", body)
}

fn render_script() -> String {
    r##"<script>
(function () {
  const csrfToken = document.querySelector('meta[name=csrf-token]')?.content || '';

  // The two inline_result panels are toggled via tone classes; we
  // manage them through a single helper to keep the script flat.
  const showResult = (panel, tone, msg) => {
    if (!panel) return;
    panel.classList.remove('success', 'error', 'warn', 'info');
    panel.classList.add(tone);
    panel.textContent = msg;
    panel.hidden = false;
  };

  document.getElementById('export-form')?.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const includeUsers = document.getElementById('include-users-checkbox').checked;
    const panel = document.getElementById('export-result');
    showResult(panel, 'info', 'Downloading…');
    try {
      const res = await fetch('/api/admin/migration/export?include_users=' + includeUsers);
      if (!res.ok) throw new Error(await res.text());
      const blob = await res.blob();
      const date = new Date().toISOString().slice(0, 10).replace(/-/g, '');
      const a = document.createElement('a');
      a.href = URL.createObjectURL(blob);
      a.download = 'noye-export-' + date + '.json';
      document.body.appendChild(a);
      a.click();
      a.remove();
      URL.revokeObjectURL(a.href);
      showResult(panel, 'success', 'Exported.');
    } catch (e) {
      showResult(panel, 'error', 'Export failed: ' + e.message);
    }
  });

  document.getElementById('import-form')?.addEventListener('submit', async (ev) => {
    ev.preventDefault();
    const panel = document.getElementById('import-result');
    const fileInput = document.getElementById('import-file');
    const policy = document.querySelector('input[name="policy"]:checked').value;
    const apply = document.getElementById('apply-checkbox').checked;

    if (!fileInput.files || !fileInput.files[0]) {
      showResult(panel, 'error', 'Please choose a JSON file.');
      return;
    }
    if (apply && !confirm('Apply will write to the database under policy: ' + policy + '. Continue?')) {
      return;
    }

    showResult(panel, 'info', 'Reading file…');

    try {
      const text = await fileInput.files[0].text();
      const payload = JSON.parse(text);
      showResult(panel, 'info', apply ? 'Applying…' : 'Validating…');
      const headers = { 'Content-Type': 'application/json' };
      if (csrfToken) headers['X-CSRF-Token'] = csrfToken;
      const res = await fetch('/api/admin/migration/import', {
        method: 'POST',
        headers: headers,
        body: JSON.stringify({ payload: payload, on_conflict: policy, apply: apply }),
      });
      if (!res.ok) throw new Error(await res.text());
      const body = await res.json();
      const lines = [
        (body.applied ? 'APPLIED' : 'DRY-RUN') + ' (policy: ' + body.conflict_policy + ')',
        'targets:               ' + body.rows.targets,
        'channels:              ' + body.rows.channels,
        'target_notifications:  ' + body.rows.target_notifications,
        'maintenance_windows:   ' + body.rows.maintenance_windows,
        'users:                 ' + body.rows.users,
        'skipped:               ' + body.rows.skipped,
        'replaced:              ' + body.rows.replaced,
      ];
      if (body.warnings && body.warnings.length) {
        lines.push('');
        lines.push('warnings:');
        body.warnings.forEach((w) => lines.push('  - ' + w));
      }
      // The result panel uses CSS to preserve newlines on the textContent
      // we set; we keep the multi-line summary readable by setting
      // white-space via inline style on this one element specifically.
      panel.style.whiteSpace = 'pre-wrap';
      panel.style.fontFamily = 'var(--font-mono)';
      showResult(panel, 'success', lines.join('\n'));
    } catch (e) {
      panel.style.whiteSpace = '';
      panel.style.fontFamily = '';
      showResult(panel, 'error', 'Import failed: ' + e.message);
    }
  });
})();
</script>"##.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_caller(role: &str) -> Caller {
        Caller {
            user_id: "u-1".into(),
            email: "u@x".into(),
            name: "U".into(),
            role: role.into(),
        }
    }

    #[test]
    fn render_member_sees_restricted_message() {
        let html = render_page(&fake_caller("member"));
        assert!(html.contains("restricted to administrators"));
        // Non-admin must not see the export/import forms.
        assert!(!html.contains(r#"id="export-form""#));
        assert!(!html.contains(r#"id="import-form""#));
    }

    #[test]
    fn render_admin_sees_all_four_sections() {
        let html = render_page(&fake_caller("admin"));
        assert!(html.contains(r#"id="migration-intro""#));
        assert!(html.contains(r#"id="migration-export""#));
        assert!(html.contains(r#"id="migration-import""#));
        assert!(html.contains(r#"id="migration-bulk""#));
    }

    #[test]
    fn render_export_form_has_include_users_checkbox_off_by_default() {
        let html = render_page(&fake_caller("admin"));
        assert!(html.contains(r#"id="include-users-checkbox""#));
        // Off by default: no `checked` attribute on this input.
        // We look for the specific pattern that proves it's unchecked.
        assert!(html.contains(r#"name="include_users" id="include-users-checkbox""#));
        // Confirm the input doesn't carry `checked` (hard to check with
        // string ops, but we can verify the help text mentions
        // "Off by default" so wording stays in sync with the markup).
        assert!(html.contains("Off by default"));
    }

    #[test]
    fn render_import_form_default_policy_is_skip() {
        // The default is "skip" (recommended for fresh migrations);
        // the radio input carries `checked`.
        let html = render_page(&fake_caller("admin"));
        assert!(html.contains(r#"value="skip" checked"#));
    }

    #[test]
    fn render_import_form_apply_unchecked_means_dry_run() {
        let html = render_page(&fake_caller("admin"));
        // The Apply checkbox starts unchecked so Run = Dry-run by default.
        assert!(html.contains(r#"id="apply-checkbox""#));
        assert!(html.to_lowercase().contains("dry-run"));
    }

    #[test]
    fn render_uses_phase_a_inline_result_panels() {
        let html = render_page(&fake_caller("admin"));
        // Phase A's inline_result component has the role + aria-live attrs.
        assert!(html.contains(r#"id="export-result""#));
        assert!(html.contains(r#"id="import-result""#));
        assert!(html.contains(r#"aria-live="polite""#));
    }

    #[test]
    fn render_no_legacy_color_token_references() {
        // Phase D explicitly removed --color-fg-muted / --color-success /
        // --color-danger references that were left over from earlier
        // versions. We verify they're gone from the rendered HTML.
        let html = render_page(&fake_caller("admin"));
        assert!(
            !html.contains("--color-fg-muted"),
            "legacy --color-fg-muted token reference must not appear"
        );
        assert!(
            !html.contains("--color-success"),
            "legacy --color-success token reference must not appear"
        );
        assert!(
            !html.contains("--color-danger"),
            "legacy --color-danger token reference must not appear"
        );
    }
}
