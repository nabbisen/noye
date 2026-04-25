use noye_shared::User;
use crate::ui::layout::escape_html;

pub fn render(users: &[User]) -> String {
    let mut html = String::new();

    // ユーザー管理セクション
    html.push_str(r#"<section aria-label="ユーザー管理">"#);
    html.push_str(r#"<div class="card">"#);
    html.push_str("<h3>User Management</h3>");

    if users.is_empty() {
        html.push_str("<p>登録済みユーザーはいません。</p>");
    } else {
        html.push_str(r#"<table aria-label="ユーザー一覧">"#);
        html.push_str("<thead><tr>");
        html.push_str("<th scope=\"col\">Name</th>");
        html.push_str("<th scope=\"col\">Email</th>");
        html.push_str("<th scope=\"col\">Role</th>");
        html.push_str("<th scope=\"col\">Active</th>");
        html.push_str("</tr></thead><tbody>");

        for user in users {
            let active_badge = if user.is_active {
                r#"<span class="badge badge-up">active</span>"#
            } else {
                r#"<span class="badge badge-unknown">inactive</span>"#
            };

            let role_badge = match user.role.as_str() {
                "admin" => r#"<span class="badge badge-maint">admin</span>"#,
                _ => r#"<span class="badge badge-unknown">member</span>"#,
            };

            html.push_str("<tr>");
            html.push_str(&format!("<td>{}</td>", escape_html(&user.name)));
            html.push_str(&format!("<td>{}</td>", escape_html(&user.email)));
            html.push_str(&format!("<td>{}</td>", role_badge));
            html.push_str(&format!("<td>{}</td>", active_badge));
            html.push_str("</tr>");
        }

        html.push_str("</tbody></table>");
    }

    html.push_str("</div>");
    html.push_str("</section>");

    // システム設定セクション
    html.push_str(r#"<section aria-label="システム設定" style="margin-top:var(--space-lg)">"#);
    html.push_str(r#"<div class="card">"#);
    html.push_str("<h3>System Settings</h3>");
    html.push_str(r#"<dl>"#);
    html.push_str(r#"<div style="display:flex;gap:var(--space-md);padding:var(--space-xs) 0;border-bottom:1px solid var(--c-border)">
        <dt style="min-width:200px;color:var(--c-text-muted);font-size:0.875rem">Authentication</dt>
        <dd style="font-size:0.875rem">汎用 OIDC クライアント (Authorization Code + PKCE)</dd>
    </div>"#);
    html.push_str(r#"<div style="display:flex;gap:var(--space-md);padding:var(--space-xs) 0;border-bottom:1px solid var(--c-border)">
        <dt style="min-width:200px;color:var(--c-text-muted);font-size:0.875rem">Bot Protection</dt>
        <dd style="font-size:0.875rem">Cloudflare Turnstile (公開フォーム限定)</dd>
    </div>"#);
    html.push_str(r#"<div style="display:flex;gap:var(--space-md);padding:var(--space-xs) 0;border-bottom:1px solid var(--c-border)">
        <dt style="min-width:200px;color:var(--c-text-muted);font-size:0.875rem">Scheduler</dt>
        <dd style="font-size:0.875rem">Cron Triggers (毎分実行、内部判定)</dd>
    </div>"#);
    html.push_str(r#"<div style="display:flex;gap:var(--space-md);padding:var(--space-xs) 0">
        <dt style="min-width:200px;color:var(--c-text-muted);font-size:0.875rem">Data Storage</dt>
        <dd style="font-size:0.875rem">D1 (正本) / KV (キャッシュ) / R2 (アーカイブ)</dd>
    </div>"#);
    html.push_str("</dl>");
    html.push_str("</div>");
    html.push_str("</section>");

    html
}
