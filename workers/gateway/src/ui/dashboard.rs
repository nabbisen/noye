use noye_shared::Incident;
use noye_shared::StatusSummary;
use crate::ui::layout::{escape_html, relative_time, status_badge};

/// ダッシュボードページのSSRレンダリング
pub fn render(summary: &StatusSummary, recent_incidents: &[Incident]) -> String {
    let mut html = String::new();

    // ステータスサマリー
    html.push_str(r#"<section aria-label="ステータスサマリー">"#);
    html.push_str(r#"<div class="summary-grid" role="list">"#);

    html.push_str(&summary_item("total", "Total", summary.total, ""));
    html.push_str(&summary_item("up", "Up", summary.up, "up"));
    html.push_str(&summary_item("down", "Down", summary.down, "down"));
    html.push_str(&summary_item("degraded", "Degraded", summary.degraded, "degraded"));
    html.push_str(&summary_item("maint", "Maintenance", summary.maintenance, ""));
    html.push_str(&summary_item("unknown", "Unknown", summary.unknown, ""));
    html.push_str(&summary_item("disabled", "Disabled", summary.disabled, ""));

    html.push_str("</div>");
    html.push_str("</section>");

    // 最近のインシデント
    html.push_str(r#"<section aria-label="最近のインシデント">"#);
    html.push_str(r#"<div class="card">"#);
    html.push_str("<h3>Recent Incidents</h3>");

    if recent_incidents.is_empty() {
        html.push_str(r#"<p role="status">現在、未解決のインシデントはありません。</p>"#);
    } else {
        html.push_str(r#"<table aria-label="インシデント一覧">"#);
        html.push_str("<thead><tr>");
        html.push_str("<th scope=\"col\">Status</th>");
        html.push_str("<th scope=\"col\">Target</th>");
        html.push_str("<th scope=\"col\">Cause</th>");
        html.push_str("<th scope=\"col\">Opened</th>");
        html.push_str("<th scope=\"col\">Duration</th>");
        html.push_str("</tr></thead><tbody>");

        for incident in recent_incidents {
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
                format_duration(incident.duration_sec)
            ));
            html.push_str("</tr>");
        }

        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");
    html.push_str("</section>");

    html
}

fn summary_item(id: &str, label: &str, value: i64, class: &str) -> String {
    let class_attr = if class.is_empty() {
        "summary-item".to_string()
    } else {
        format!("summary-item {}", class)
    };
    format!(
        r#"<div class="{class}" role="listitem" aria-label="{label}: {value}">
            <div class="value" id="summary-{id}">{value}</div>
            <div class="label">{label}</div>
        </div>"#,
        class = class_attr,
        id = id,
        label = label,
        value = value,
    )
}

fn format_duration(seconds: Option<i64>) -> String {
    match seconds {
        Some(s) if s >= 3600 => format!("{}h {}m", s / 3600, (s % 3600) / 60),
        Some(s) if s >= 60 => format!("{}m {}s", s / 60, s % 60),
        Some(s) => format!("{}s", s),
        None => "-".to_string(),
    }
}
