use noye_shared::AuditEntry;
use crate::ui::layout::{escape_html, relative_time};

pub fn render_list(entries: &[AuditEntry]) -> String {
    let mut html = String::new();

    html.push_str(r#"<div class="card">"#);

    if entries.is_empty() {
        html.push_str("<p>監査ログはありません。</p>");
    } else {
        html.push_str(r#"<table aria-label="監査ログ">"#);
        html.push_str("<thead><tr>");
        html.push_str("<th scope=\"col\">Time</th>");
        html.push_str("<th scope=\"col\">Actor</th>");
        html.push_str("<th scope=\"col\">Action</th>");
        html.push_str("<th scope=\"col\">Resource</th>");
        html.push_str("<th scope=\"col\">Result</th>");
        html.push_str("</tr></thead><tbody>");

        for entry in entries {
            let result_class = if entry.result == "success" {
                "badge-up"
            } else {
                "badge-down"
            };

            html.push_str("<tr>");
            html.push_str(&format!("<td>{}</td>", relative_time(&entry.action_time)));
            html.push_str(&format!(
                "<td>{}</td>",
                escape_html(entry.actor_email.as_deref().unwrap_or(&entry.actor_id))
            ));
            html.push_str(&format!("<td>{}</td>", escape_html(&entry.action_type)));
            html.push_str(&format!(
                "<td>{}: {}</td>",
                escape_html(&entry.resource_type),
                escape_html(entry.resource_id.as_deref().unwrap_or("-"))
            ));
            html.push_str(&format!(
                r#"<td><span class="badge {}">{}</span></td>"#,
                result_class,
                escape_html(&entry.result)
            ));
            html.push_str("</tr>");
        }

        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");
    html
}
