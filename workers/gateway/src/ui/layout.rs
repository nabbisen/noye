use noye_shared::Caller;

/// ページ共通レイアウト (ABDD: Accessible by Default and by Design)
///
/// セマンティックHTML、キーボードナビゲーション完全対応、
/// WAI-ARIA適切付与、CSSなしでもステータスが把握可能なHTMLファーストSSR。
pub fn wrap(title: &str, caller: &Caller, content: &str) -> String {
    format!(
        r##"<!DOCTYPE html>
<html lang="ja">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{title} - Noye Monitor</title>
    <style>{CSS}</style>
</head>
<body>
    <a href="#main-content" class="skip-link">メインコンテンツへスキップ</a>

    <header role="banner">
        <div class="header-inner">
            <h1 class="logo">
                <a href="/" aria-label="Noye Monitor ダッシュボードへ">Noye</a>
            </h1>
            <nav role="navigation" aria-label="メインナビゲーション">
                <ul>
                    <li><a href="/" aria-current="{dash_current}">Dashboard</a></li>
                    <li><a href="/targets" aria-current="{targets_current}">Targets</a></li>
                    <li><a href="/incidents" aria-current="{incidents_current}">Incidents</a></li>
                    <li><a href="/maintenance" aria-current="{maint_current}">Maintenance</a></li>
                    {admin_nav}
                </ul>
            </nav>
            <div class="user-info" aria-label="ユーザー情報">
                <span>{caller_name}</span>
                <span class="role-badge" aria-label="権限: {caller_role}">{caller_role}</span>
                <a href="/auth/logout" class="logout-link" aria-label="ログアウト">ログアウト</a>
            </div>
        </div>
    </header>

    <main id="main-content" role="main" tabindex="-1">
        <div class="container">
            <h2 class="page-title">{title}</h2>
            {content}
        </div>
    </main>

    <footer role="contentinfo">
        <p>Noye Monitor v0.1.0 &mdash; Accessible by Default and by Design</p>
    </footer>
</body>
</html>"##,
        title = escape_html(title),
        CSS = CSS,
        dash_current = if title == "Dashboard" { "page" } else { "false" },
        targets_current = if title == "Targets" { "page" } else { "false" },
        incidents_current = if title == "Incidents" { "page" } else { "false" },
        maint_current = if title == "Maintenance" { "page" } else { "false" },
        admin_nav = if caller.is_admin() {
            r#"<li><a href="/audit">Audit Log</a></li>
               <li><a href="/settings">Settings</a></li>"#
        } else {
            ""
        },
        caller_name = escape_html(&caller.name),
        caller_role = escape_html(&caller.role),
        content = content,
    )
}

/// HTMLエスケープ
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

/// ステータスのアクセシブルなバッジを生成
pub fn status_badge(status: &str) -> String {
    let (class, label) = match status {
        "up" => ("badge-up", "正常稼働中"),
        "down" => ("badge-down", "障害発生中"),
        "degraded" => ("badge-degraded", "性能低下"),
        "maintenance" => ("badge-maint", "メンテナンス中"),
        "unknown" => ("badge-unknown", "未確認"),
        "open" => ("badge-down", "未解決"),
        "resolved" => ("badge-up", "解決済み"),
        "acknowledged" => ("badge-degraded", "確認済み"),
        _ => ("badge-unknown", "不明"),
    };
    format!(
        r#"<span class="badge {class}" role="status" aria-label="{label}">{status}</span>"#,
        class = class,
        label = label,
        status = escape_html(status),
    )
}

/// 時間の相対表示
pub fn relative_time(timestamp: &str) -> String {
    format!(
        r#"<time datetime="{ts}">{ts}</time>"#,
        ts = escape_html(timestamp),
    )
}

const CSS: &str = r#"
/* ── リセットと基本設定 ── */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }
:root {
    --c-bg: #0f1117;
    --c-surface: #1a1d27;
    --c-border: #2a2d3a;
    --c-text: #e4e6ef;
    --c-text-muted: #8b8fa3;
    --c-primary: #6c8cff;
    --c-up: #34d399;
    --c-down: #f87171;
    --c-degraded: #fbbf24;
    --c-maint: #a78bfa;
    --c-unknown: #6b7280;
    --c-focus: #6c8cff;
    --radius: 6px;
    --space-xs: 0.25rem;
    --space-sm: 0.5rem;
    --space-md: 1rem;
    --space-lg: 1.5rem;
    --space-xl: 2rem;
}
@media (prefers-color-scheme: light) {
    :root {
        --c-bg: #f5f6fa;
        --c-surface: #ffffff;
        --c-border: #e2e4ec;
        --c-text: #1a1d27;
        --c-text-muted: #6b7280;
    }
}
html { font-size: 16px; }
body {
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    background: var(--c-bg);
    color: var(--c-text);
    line-height: 1.6;
    min-height: 100vh;
    display: flex;
    flex-direction: column;
}

/* ── スキップリンク (アクセシビリティ) ── */
.skip-link {
    position: absolute;
    top: -100%;
    left: var(--space-md);
    background: var(--c-primary);
    color: #fff;
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius);
    z-index: 1000;
    text-decoration: none;
    font-weight: 600;
}
.skip-link:focus {
    top: var(--space-md);
    outline: 3px solid var(--c-focus);
    outline-offset: 2px;
}

/* ── フォーカスリング ── */
:focus-visible {
    outline: 3px solid var(--c-focus);
    outline-offset: 2px;
}

/* ── ヘッダー ── */
header {
    background: var(--c-surface);
    border-bottom: 1px solid var(--c-border);
    padding: var(--space-sm) var(--space-lg);
}
.header-inner {
    max-width: 1200px;
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-lg);
    flex-wrap: wrap;
}
.logo a {
    color: var(--c-primary);
    text-decoration: none;
    font-size: 1.25rem;
    font-weight: 700;
    letter-spacing: -0.02em;
}
nav ul {
    display: flex;
    list-style: none;
    gap: var(--space-xs);
}
nav a {
    display: block;
    padding: var(--space-xs) var(--space-sm);
    color: var(--c-text-muted);
    text-decoration: none;
    border-radius: var(--radius);
    font-size: 0.875rem;
    transition: background 0.15s, color 0.15s;
}
nav a:hover, nav a[aria-current="page"] {
    background: var(--c-bg);
    color: var(--c-text);
}
.user-info {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    font-size: 0.875rem;
    color: var(--c-text-muted);
}
.logout-link {
    padding: var(--space-xs) var(--space-sm);
    color: var(--c-text-muted);
    text-decoration: none;
    border-radius: var(--radius);
    font-size: 0.8125rem;
    border: 1px solid var(--c-border);
    transition: background 0.15s, color 0.15s;
}
.logout-link:hover {
    background: var(--c-bg);
    color: var(--c-text);
}

/* ── メインコンテンツ ── */
main { flex: 1; padding: var(--space-xl) var(--space-lg); }
.container { max-width: 1200px; margin: 0 auto; }
.page-title {
    font-size: 1.5rem;
    font-weight: 600;
    margin-bottom: var(--space-lg);
}

/* ── カード ── */
.card {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: var(--radius);
    padding: var(--space-lg);
    margin-bottom: var(--space-md);
}
.card h3 { font-size: 1rem; font-weight: 600; margin-bottom: var(--space-sm); }

/* ── サマリーグリッド ── */
.summary-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: var(--space-md);
    margin-bottom: var(--space-xl);
}
.summary-item {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: var(--radius);
    padding: var(--space-md);
    text-align: center;
}
.summary-item .value {
    font-size: 2rem;
    font-weight: 700;
    line-height: 1.2;
}
.summary-item .label {
    font-size: 0.75rem;
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
}
.summary-item.up .value { color: var(--c-up); }
.summary-item.down .value { color: var(--c-down); }
.summary-item.degraded .value { color: var(--c-degraded); }

/* ── バッジ ── */
.badge {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
}
.badge-up { background: #064e3b; color: var(--c-up); }
.badge-down { background: #7f1d1d; color: var(--c-down); }
.badge-degraded { background: #78350f; color: var(--c-degraded); }
.badge-maint { background: #312e81; color: var(--c-maint); }
.badge-unknown { background: #1f2937; color: var(--c-unknown); }
.role-badge {
    display: inline-block;
    padding: 0.125rem 0.375rem;
    border-radius: var(--radius);
    font-size: 0.6875rem;
    background: var(--c-bg);
    color: var(--c-text-muted);
    text-transform: uppercase;
}
@media (prefers-color-scheme: light) {
    .badge-up { background: #d1fae5; color: #065f46; }
    .badge-down { background: #fee2e2; color: #991b1b; }
    .badge-degraded { background: #fef3c7; color: #92400e; }
    .badge-maint { background: #ede9fe; color: #5b21b6; }
    .badge-unknown { background: #f3f4f6; color: #4b5563; }
}

/* ── テーブル ── */
table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.875rem;
}
th, td {
    padding: var(--space-sm) var(--space-md);
    text-align: left;
    border-bottom: 1px solid var(--c-border);
}
th {
    font-weight: 600;
    color: var(--c-text-muted);
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
}
tbody tr:hover { background: var(--c-bg); }

/* ── フッター ── */
footer {
    padding: var(--space-lg);
    text-align: center;
    font-size: 0.75rem;
    color: var(--c-text-muted);
    border-top: 1px solid var(--c-border);
}

/* ── レスポンシブ ── */
@media (max-width: 768px) {
    .header-inner { gap: var(--space-sm); }
    nav ul { flex-wrap: wrap; }
    .summary-grid { grid-template-columns: repeat(2, 1fr); }
    table { display: block; overflow-x: auto; }
}

/* ── 削減モーション対応 ── */
@media (prefers-reduced-motion: reduce) {
    * { transition: none !important; animation: none !important; }
}
"#;
