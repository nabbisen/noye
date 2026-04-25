use noye_shared::Caller;
use noye_shared::CheckResult;
use noye_shared::TargetState;
use noye_shared::Target;
use crate::ui::layout::{escape_html, relative_time, status_badge};

/// 監視対象一覧ページ
pub fn render_list(targets: &[Target], states: &[TargetState], caller: &Caller) -> String {
    let mut html = String::new();

    if caller.is_admin() {
        html.push_str(r#"<div class="card" style="margin-bottom:var(--space-lg)">"#);
        html.push_str(r#"<details>"#);
        html.push_str(r#"<summary><strong>+ Add New Target</strong></summary>"#);
        html.push_str(r#"<div style="margin-top:var(--space-md)">"#);
        html.push_str(r#"<p>Use the API to create a new target:</p>"#);
        html.push_str(r#"<code>POST /api/targets</code> with JSON body."#);
        html.push_str("</div></details></div>");
    }

    if targets.is_empty() {
        html.push_str(r#"<div class="card"><p>監視対象が登録されていません。</p></div>"#);
        return html;
    }

    html.push_str(r#"<div class="card">"#);
    html.push_str(r#"<table aria-label="監視対象一覧">"#);
    html.push_str("<thead><tr>");
    html.push_str("<th scope=\"col\">Status</th>");
    html.push_str("<th scope=\"col\">Name</th>");
    html.push_str("<th scope=\"col\">Type</th>");
    html.push_str("<th scope=\"col\">Host</th>");
    html.push_str("<th scope=\"col\">Interval</th>");
    html.push_str("<th scope=\"col\">Last Check</th>");
    html.push_str("</tr></thead><tbody>");

    for target in targets {
        let state = states
            .iter()
            .find(|s| s.target_id == target.id);
        let status = state
            .map(|s| s.current_status.as_str())
            .unwrap_or("unknown");
        let last_checked = state
            .and_then(|s| s.last_checked_at.as_deref())
            .unwrap_or("-");

        let disabled_attr = if target.is_disabled {
            r#" class="disabled" aria-disabled="true""#
        } else {
            ""
        };

        html.push_str(&format!("<tr{}>", disabled_attr));
        html.push_str(&format!("<td>{}</td>", status_badge(status)));
        html.push_str(&format!(
            r#"<td><a href="/targets/{id}">{name}</a>{disabled}</td>"#,
            id = escape_html(&target.id),
            name = escape_html(&target.name),
            disabled = if target.is_disabled {
                r#" <span class="badge badge-unknown">disabled</span>"#
            } else {
                ""
            },
        ));
        html.push_str(&format!("<td>{}</td>", escape_html(&target.target_type)));
        html.push_str(&format!(
            "<td>{}{}</td>",
            escape_html(&target.host),
            target
                .port
                .map(|p| format!(":{}", p))
                .unwrap_or_default()
        ));
        html.push_str(&format!("<td>{}m</td>", target.interval_minutes));
        html.push_str(&format!("<td>{}</td>", relative_time(last_checked)));
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table>");
    html.push_str("</div>");

    html
}

/// 監視対象詳細ページ
pub fn render_detail(
    target: &Target,
    state: &TargetState,
    results: &[CheckResult],
) -> String {
    let mut html = String::new();

    // 基本情報カード
    html.push_str(r#"<section aria-label="対象情報">"#);
    html.push_str(r#"<div class="card">"#);
    html.push_str("<h3>Target Information</h3>");
    html.push_str(r#"<dl>"#);

    html.push_str(&dl_row("Status", &status_badge(&state.current_status)));
    html.push_str(&dl_row("Type", &escape_html(&target.target_type)));
    html.push_str(&dl_row(
        "Host",
        &format!(
            "{}{}{}",
            escape_html(&target.host),
            target.port.map(|p| format!(":{}", p)).unwrap_or_default(),
            target.path.as_deref().unwrap_or(""),
        ),
    ));
    html.push_str(&dl_row(
        "Expected Status",
        &target
            .expected_status
            .map(|s| s.to_string())
            .unwrap_or_else(|| "200".to_string()),
    ));
    html.push_str(&dl_row("Timeout", &format!("{}s", target.timeout_sec)));
    html.push_str(&dl_row("Retries", &target.retry_count.to_string()));
    html.push_str(&dl_row("Interval", &format!("{}m", target.interval_minutes)));
    html.push_str(&dl_row(
        "Consecutive Successes",
        &state.consecutive_successes.to_string(),
    ));
    html.push_str(&dl_row(
        "Consecutive Failures",
        &state.consecutive_failures.to_string(),
    ));
    html.push_str(&dl_row(
        "Last Checked",
        &state
            .last_checked_at
            .as_deref()
            .map(|t| relative_time(t))
            .unwrap_or_else(|| "-".to_string()),
    ));

    if let Some(ref tags) = target.tags {
        html.push_str(&dl_row("Tags", &escape_html(tags)));
    }

    html.push_str("</dl>");
    html.push_str("</div>");
    html.push_str("</section>");

    // チェック結果履歴
    html.push_str(r#"<section aria-label="チェック結果履歴">"#);
    html.push_str(r#"<div class="card">"#);
    html.push_str("<h3>Recent Check Results</h3>");

    if results.is_empty() {
        html.push_str("<p>まだチェック結果がありません。</p>");
    } else {
        html.push_str(r#"<table aria-label="チェック結果">"#);
        html.push_str("<thead><tr>");
        html.push_str("<th scope=\"col\">Result</th>");
        html.push_str("<th scope=\"col\">Status Code</th>");
        html.push_str("<th scope=\"col\">Response Time</th>");
        html.push_str("<th scope=\"col\">Checked At</th>");
        html.push_str("<th scope=\"col\">Error</th>");
        html.push_str("</tr></thead><tbody>");

        for result in results {
            let result_badge = if result.is_success {
                r#"<span class="badge badge-up" role="status">OK</span>"#
            } else {
                r#"<span class="badge badge-down" role="status">FAIL</span>"#
            };

            html.push_str("<tr>");
            html.push_str(&format!("<td>{}</td>", result_badge));
            html.push_str(&format!(
                "<td>{}</td>",
                result
                    .status_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-".to_string())
            ));
            html.push_str(&format!(
                "<td>{}ms</td>",
                result.response_time_ms.unwrap_or(0)
            ));
            html.push_str(&format!("<td>{}</td>", relative_time(&result.checked_at)));
            html.push_str(&format!(
                "<td>{}</td>",
                escape_html(result.error_message.as_deref().unwrap_or("-"))
            ));
            html.push_str("</tr>");
        }

        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");
    html.push_str("</section>");

    html
}

fn dl_row(label: &str, value: &str) -> String {
    format!(
        r#"<div style="display:flex;gap:var(--space-md);padding:var(--space-xs) 0;border-bottom:1px solid var(--c-border)">
            <dt style="min-width:160px;color:var(--c-text-muted);font-size:0.875rem">{}</dt>
            <dd style="font-size:0.875rem">{}</dd>
        </div>"#,
        label, value,
    )
}
