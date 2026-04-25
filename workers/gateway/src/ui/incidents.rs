use noye_shared::Incident;
use crate::ui::layout::{escape_html, relative_time, status_badge};

pub fn render_list(incidents: &[Incident]) -> String {
    let mut html = String::new();

    html.push_str(r#"<div class="card">"#);

    if incidents.is_empty() {
        html.push_str(r#"<p role="status">インシデントはありません。</p>"#);
    } else {
        let open_count = incidents.iter().filter(|i| i.status == "open").count();
        if open_count > 0 {
            html.push_str(&format!(
                r#"<p role="alert" class="badge badge-down" style="margin-bottom:var(--space-md);display:inline-block">
                    未解決のインシデント: {}件
                </p>"#,
                open_count
            ));
        }

        html.push_str(r#"<table aria-label="インシデント一覧">"#);
        html.push_str("<thead><tr>");
        html.push_str("<th scope=\"col\">Status</th>");
        html.push_str("<th scope=\"col\">Target</th>");
        html.push_str("<th scope=\"col\">Cause</th>");
        html.push_str("<th scope=\"col\">Opened</th>");
        html.push_str("<th scope=\"col\">Resolved</th>");
        html.push_str("<th scope=\"col\">Duration</th>");
        html.push_str("<th scope=\"col\">Actions</th>");
        html.push_str("</tr></thead><tbody>");

        for incident in incidents {
            html.push_str("<tr>");
            html.push_str(&format!("<td>{}</td>", status_badge(&incident.status)));
            html.push_str(&format!("<td>{}</td>", escape_html(&incident.target_id)));
            html.push_str(&format!(
                "<td>{}</td>",
                escape_html(incident.cause.as_deref().unwrap_or("-"))
            ));
            html.push_str(&format!("<td>{}</td>", relative_time(&incident.opened_at)));
            html.push_str(&format!(
                "<td>{}</td>",
                incident
                    .resolved_at
                    .as_deref()
                    .map(|t| relative_time(t))
                    .unwrap_or_else(|| "-".to_string())
            ));
            html.push_str(&format!(
                "<td>{}</td>",
                format_duration(incident.duration_sec)
            ));

            if incident.status == "open" {
                html.push_str(&format!(
                    r#"<td><button onclick="resolveIncident('{}')"
                        aria-label="{} を手動復旧する">Resolve</button></td>"#,
                    escape_html(&incident.id),
                    escape_html(&incident.target_id),
                ));
            } else {
                html.push_str("<td>-</td>");
            }
            html.push_str("</tr>");
        }

        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");

    // 手動復旧用のスクリプト (Progressive Enhancement)
    html.push_str(r#"<script>
async function resolveIncident(id) {
    if (!confirm('このインシデントを手動復旧しますか？')) return;
    const note = prompt('復旧メモ (任意):');
    const res = await fetch('/api/incidents/' + id + '/resolve', {
        method: 'POST',
        headers: {'Content-Type': 'application/json'},
        body: JSON.stringify({note: note || null})
    });
    if (res.ok) { location.reload(); }
    else { alert('復旧処理に失敗しました'); }
}
</script>"#);

    html
}

fn format_duration(seconds: Option<i64>) -> String {
    match seconds {
        Some(s) if s >= 3600 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        Some(s) if s >= 60 => format!("{}m {}s", s / 60, s % 60),
        Some(s) => format!("{}s", s),
        None => "-".to_string(),
    }
}
