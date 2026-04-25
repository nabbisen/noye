use noye_shared::Caller;
use noye_shared::MaintenanceWindow;
use crate::ui::layout::{escape_html, relative_time};

pub fn render_list(windows: &[MaintenanceWindow], caller: &Caller) -> String {
    let mut html = String::new();

    if caller.is_admin() {
        html.push_str(r#"<div class="card" style="margin-bottom:var(--space-lg)">"#);
        html.push_str(r#"<details>"#);
        html.push_str(r#"<summary><strong>+ Create Maintenance Window</strong></summary>"#);
        html.push_str(r#"<div style="margin-top:var(--space-md)">"#);
        html.push_str(r#"<p>Use the API to schedule maintenance:</p>"#);
        html.push_str(r#"<code>POST /api/maintenance</code> with JSON body including name, start_at, end_at, and optional target_tag or target_id."#);
        html.push_str("</div></details></div>");
    }

    html.push_str(r#"<div class="card">"#);

    if windows.is_empty() {
        html.push_str("<p>メンテナンス期間は設定されていません。</p>");
    } else {
        html.push_str(r#"<table aria-label="メンテナンス期間一覧">"#);
        html.push_str("<thead><tr>");
        html.push_str("<th scope=\"col\">Name</th>");
        html.push_str("<th scope=\"col\">Start</th>");
        html.push_str("<th scope=\"col\">End</th>");
        html.push_str("<th scope=\"col\">Scope</th>");
        html.push_str("<th scope=\"col\">Suppress Notify</th>");
        html.push_str("<th scope=\"col\">Created By</th>");
        html.push_str("</tr></thead><tbody>");

        for mw in windows {
            let scope = if let Some(ref tid) = mw.target_id {
                format!("Target: {}", escape_html(tid))
            } else if let Some(ref tag) = mw.target_tag {
                format!("Tag: {}", escape_html(tag))
            } else {
                "All targets".to_string()
            };

            html.push_str("<tr>");
            html.push_str(&format!("<td>{}</td>", escape_html(&mw.name)));
            html.push_str(&format!("<td>{}</td>", relative_time(&mw.start_at)));
            html.push_str(&format!("<td>{}</td>", relative_time(&mw.end_at)));
            html.push_str(&format!("<td>{}</td>", scope));
            html.push_str(&format!(
                "<td>{}</td>",
                if mw.suppress_notify { "Yes" } else { "No" }
            ));
            html.push_str(&format!("<td>{}</td>", escape_html(&mw.created_by)));
            html.push_str("</tr>");
        }

        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");
    html
}
