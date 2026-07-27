//! Design tokens and CSS for the Noye UI.
//!
//! ## Token philosophy
//!
//! All visible color, spacing, radius, and typography is exposed as a CSS
//! custom property under `:root` (the dark theme is the baseline). The
//! `prefers-color-scheme: light` block overrides the same names for the
//! light theme, so component CSS only ever references token names — never
//! hex codes — and automatically tracks the active theme.
//!
//! Re-using tokens forces visual consistency across pages: every "card"
//! looks identical because they all reach for `--c-surface` and
//! `--space-lg`. A new contributor adding a panel doesn't need to invent
//! a new background color or a new padding value; they just reach for an
//! existing token.
//!
//! ## Why these specific tokens
//!
//! | Concern | Token group | Notes |
//! |---|---|---|
//! | Page chrome | `--c-bg` / `--c-surface` / `--c-surface-2` | Three depth levels: page, card, raised (hover, sticky header backdrop) |
//! | Text hierarchy | `--c-text` / `--c-text-muted` / `--c-text-quiet` | Primary / secondary / disabled-or-meta. All three meet WCAG AA against `--c-bg` and `--c-surface` |
//! | Status | `--c-{up,down,degraded,maint,unknown,info}` plus `*-bg` | Semantic, not visual: a status badge always picks the correct fg/bg pair, no per-component hex |
//! | Spacing | `--space-2xs` … `--space-2xl` | 8-pt grid. Use `--space-md` for default gaps; smaller values are deliberate compactions |
//! | Radius | `--radius-sm` / `--radius-md` / `--radius-pill` | Small for inline elements (badges), medium for cards, pill for fully-rounded |
//! | Focus | `--c-focus` + `--focus-ring` | Single source of truth for the keyboard focus ring |
//!
//! ## ABDD
//!
//! - All token color pairs (`--c-text` on `--c-surface`, `--c-text-muted`
//!   on `--c-bg`, etc.) are checked against WCAG AA in
//!   `crate::ui::layout::contrast::tests`. The check pins the contrast
//!   ratio so accidentally lowering it (e.g. by tweaking
//!   `--c-text-muted` lighter) breaks a unit test before the deploy.
//! - `:focus-visible` always shows a 2px ring in `--c-focus` regardless
//!   of the surface beneath it; the ring color is chosen to clear AA
//!   against both `--c-bg` and `--c-surface`.
//! - `prefers-reduced-motion` disables every transition/animation.
//!
//! ## Roadmap
//!
//! - **Manual theme toggle** with cookie persistence is intentionally
//!   deferred; today's behaviour is "follow the OS". See `ROADMAP.md`
//!   for the planned increment.
//! - **High-contrast mode** as a separate token preset is also deferred.

pub const CSS: &str = r#"
/* ──────────────────────────────────────────────────────────────────
   1. Reset
   ────────────────────────────────────────────────────────────────── */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* ──────────────────────────────────────────────────────────────────
   2. Design tokens
   ────────────────────────────────────────────────────────────────── */

:root {
    /* surfaces (depth 0/1/2) */
    --c-bg: #0f1117;
    --c-surface: #1a1d27;
    --c-surface-2: #232734;

    /* borders */
    --c-border: #2a2d3a;
    --c-border-strong: #3a3f4f;

    /* text (3-level hierarchy, all AA against bg & surface) */
    --c-text: #e4e6ef;
    --c-text-muted: #a1a5b8;
    --c-text-quiet: #71758a;

    /* primary action */
    --c-primary: #7a98ff;
    --c-primary-hover: #94adff;
    --c-primary-text: #0f1117;

    /* status (foreground colors) */
    --c-up: #4ade80;
    --c-down: #f87171;
    --c-degraded: #fbbf24;
    --c-maint: #c4b5fd;
    --c-unknown: #94a3b8;
    --c-info: #7dd3fc;

    /* status (background pairs for badges; AA against the fg above) */
    --c-up-bg: #052e1a;
    --c-down-bg: #4a1313;
    --c-degraded-bg: #4a2e08;
    --c-maint-bg: #2a1f5c;
    --c-unknown-bg: #1f2937;
    --c-info-bg: #1e3a52;

    /* danger (destructive actions, separate from status "down") */
    --c-danger: #f87171;
    --c-danger-bg: #4a1313;
    --c-danger-text: #fee2e2;

    /* focus */
    --c-focus: #94adff;
    --focus-ring: 0 0 0 2px var(--c-focus);

    /* spacing (8-pt grid) */
    --space-2xs: 0.125rem;
    --space-xs: 0.25rem;
    --space-sm: 0.5rem;
    --space-md: 1rem;
    --space-lg: 1.5rem;
    --space-xl: 2rem;
    --space-2xl: 3rem;

    /* radius */
    --radius-sm: 4px;
    --radius-md: 8px;
    --radius-pill: 9999px;

    /* typography */
    --font-sans: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
    --font-mono: ui-monospace, SFMono-Regular, "SF Mono", Menlo, Monaco, Consolas, monospace;
    --fs-xs: 0.75rem;
    --fs-sm: 0.875rem;
    --fs-md: 1rem;
    --fs-lg: 1.25rem;
    --fs-xl: 1.5rem;
    --fs-2xl: 2rem;

    /* layout */
    --container-max: 1200px;
}

@media (prefers-color-scheme: light) {
    :root {
        --c-bg: #f5f6fa;
        --c-surface: #ffffff;
        --c-surface-2: #eef0f7;
        --c-border: #d8dbe5;
        --c-border-strong: #b8bcc8;
        --c-text: #1a1d27;
        --c-text-muted: #4b5163;
        --c-text-quiet: #6b7280;
        --c-primary: #3b5bdb;
        --c-primary-hover: #4c6ff0;
        --c-primary-text: #ffffff;
        --c-up: #166534;
        --c-down: #991b1b;
        --c-degraded: #92400e;
        --c-maint: #5b21b6;
        --c-unknown: #4b5563;
        --c-info: #075985;
        --c-up-bg: #d1fae5;
        --c-down-bg: #fee2e2;
        --c-degraded-bg: #fef3c7;
        --c-maint-bg: #ede9fe;
        --c-unknown-bg: #f3f4f6;
        --c-info-bg: #e0f2fe;
        --c-danger: #991b1b;
        --c-danger-bg: #fee2e2;
        --c-danger-text: #7f1d1d;
        --c-focus: #3b5bdb;
    }
}

/* ──────────────────────────────────────────────────────────────────
   3. Base
   ────────────────────────────────────────────────────────────────── */

html { font-size: 16px; }

body {
    font-family: var(--font-sans);
    font-size: var(--fs-md);
    line-height: 1.6;
    background: var(--c-bg);
    color: var(--c-text);
    min-height: 100vh;
    display: flex;
    flex-direction: column;
}

/* Skip link for keyboard users (ABDD baseline). */
.skip-link {
    position: absolute;
    top: -100%;
    left: var(--space-md);
    background: var(--c-primary);
    color: var(--c-primary-text);
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius-sm);
    z-index: 1000;
    text-decoration: none;
    font-weight: 600;
}
.skip-link:focus {
    top: var(--space-md);
}

/* Single focus-ring style for the whole UI. */
:focus-visible {
    outline: 2px solid var(--c-focus);
    outline-offset: 2px;
    border-radius: var(--radius-sm);
}

/* ──────────────────────────────────────────────────────────────────
   4. Header / chrome
   ────────────────────────────────────────────────────────────────── */

header[role="banner"] {
    background: var(--c-surface);
    border-bottom: 1px solid var(--c-border);
    padding: var(--space-sm) var(--space-lg);
}
.header-inner {
    max-width: var(--container-max);
    margin: 0 auto;
    display: flex;
    align-items: center;
    gap: var(--space-lg);
    flex-wrap: wrap;
}
.logo a {
    color: var(--c-primary);
    text-decoration: none;
    font-size: var(--fs-lg);
    font-weight: 700;
    letter-spacing: -0.02em;
}

/* Three-verb navigation: 見る / 直す / 証明する. The groups are
   semantic (a <ul> per verb) and the heading is visually-hidden so
   screen readers announce it without taking visual space. */
nav[aria-label="Main navigation"] { display: flex; gap: var(--space-md); flex-wrap: wrap; }
.nav-group { display: flex; flex-direction: column; gap: 0; }
.nav-group-label {
    font-size: var(--fs-xs);
    color: var(--c-text-quiet);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 0 var(--space-sm) var(--space-2xs);
    font-weight: 600;
}
.nav-group ul {
    display: flex;
    list-style: none;
    gap: var(--space-2xs);
}
.nav-group a {
    display: block;
    padding: var(--space-xs) var(--space-sm);
    color: var(--c-text-muted);
    text-decoration: none;
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
    transition: background 0.15s, color 0.15s;
}
.nav-group a:hover { background: var(--c-bg); color: var(--c-text); }
.nav-group a[aria-current="page"] {
    background: var(--c-bg);
    color: var(--c-text);
    font-weight: 600;
}

/* User chip on the right. */
.user-info {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: var(--space-sm);
    font-size: var(--fs-sm);
    color: var(--c-text-muted);
    flex-wrap: wrap;
}
.user-info a {
    padding: var(--space-xs) var(--space-sm);
    color: var(--c-text-muted);
    text-decoration: none;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    border: 1px solid var(--c-border);
    transition: background 0.15s, color 0.15s, border-color 0.15s;
}
.user-info a:hover { background: var(--c-bg); color: var(--c-text); border-color: var(--c-border-strong); }

/* ──────────────────────────────────────────────────────────────────
   5. Main content
   ────────────────────────────────────────────────────────────────── */

main { flex: 1; padding: var(--space-xl) var(--space-lg); }
.container { max-width: var(--container-max); margin: 0 auto; }
.page-title {
    font-size: var(--fs-xl);
    font-weight: 600;
    margin-bottom: var(--space-lg);
    line-height: 1.2;
}

/* ──────────────────────────────────────────────────────────────────
   6. Card (the most-used component)
   ────────────────────────────────────────────────────────────────── */

.card {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    padding: var(--space-lg);
    margin-bottom: var(--space-md);
}
.card > h3 {
    font-size: var(--fs-md);
    font-weight: 600;
    margin-bottom: var(--space-md);
}
.card > h3:not(:first-child) { margin-top: var(--space-lg); }
.card p + p { margin-top: var(--space-sm); }

/* ──────────────────────────────────────────────────────────────────
   7. Metric card (Dashboard)
   ────────────────────────────────────────────────────────────────── */

.metric-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: var(--space-md);
    margin-bottom: var(--space-lg);
}
.metric-card {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: var(--radius-md);
    padding: var(--space-md) var(--space-lg);
}
.metric-card .metric-label {
    font-size: var(--fs-xs);
    color: var(--c-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    margin-bottom: var(--space-xs);
}
.metric-card .metric-value {
    font-size: var(--fs-2xl);
    font-weight: 700;
    line-height: 1.1;
    color: var(--c-text);
}
.metric-card .metric-hint {
    font-size: var(--fs-xs);
    color: var(--c-text-quiet);
    margin-top: var(--space-xs);
}
.metric-card.up .metric-value { color: var(--c-up); }
.metric-card.down .metric-value { color: var(--c-down); }
.metric-card.degraded .metric-value { color: var(--c-degraded); }

/* Backwards compat: existing pages still emit `.summary-grid` / `.summary-item`.
   Map them onto the new metric-* visuals so this Phase doesn't break old
   pages while later Phases migrate them over. */
.summary-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: var(--space-md); margin-bottom: var(--space-lg); }
.summary-item { background: var(--c-surface); border: 1px solid var(--c-border); border-radius: var(--radius-md); padding: var(--space-md) var(--space-lg); text-align: center; }
.summary-item .value { font-size: var(--fs-2xl); font-weight: 700; line-height: 1.1; color: var(--c-text); }
.summary-item .label { font-size: var(--fs-xs); color: var(--c-text-muted); text-transform: uppercase; letter-spacing: 0.05em; }
.summary-item.up .value { color: var(--c-up); }
.summary-item.down .value { color: var(--c-down); }
.summary-item.degraded .value { color: var(--c-degraded); }

/* ──────────────────────────────────────────────────────────────────
   8. Status badge
   ────────────────────────────────────────────────────────────────── */

.badge {
    display: inline-flex;
    align-items: center;
    gap: var(--space-xs);
    padding: 0.125rem 0.5rem;
    border-radius: var(--radius-pill);
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    line-height: 1.4;
}
.badge::before {
    content: "";
    display: inline-block;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: currentColor;
    flex-shrink: 0;
}
.badge-up { background: var(--c-up-bg); color: var(--c-up); }
.badge-down { background: var(--c-down-bg); color: var(--c-down); }
.badge-degraded { background: var(--c-degraded-bg); color: var(--c-degraded); }
.badge-maint { background: var(--c-maint-bg); color: var(--c-maint); }
.badge-unknown { background: var(--c-unknown-bg); color: var(--c-unknown); }
.badge-info { background: var(--c-info-bg); color: var(--c-info); }
.role-badge {
    display: inline-block;
    padding: 0.125rem 0.5rem;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    background: var(--c-bg);
    color: var(--c-text-muted);
    text-transform: uppercase;
    border: 1px solid var(--c-border);
    letter-spacing: 0.03em;
}

/* ──────────────────────────────────────────────────────────────────
   9. Buttons
   ────────────────────────────────────────────────────────────────── */

.btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-xs);
    padding: var(--space-sm) var(--space-md);
    font-size: var(--fs-sm);
    font-weight: 600;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    cursor: pointer;
    text-decoration: none;
    line-height: 1.4;
    transition: background 0.15s, border-color 0.15s, color 0.15s;
    font-family: inherit;
}
.btn:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-primary { background: var(--c-primary); color: var(--c-primary-text); }
.btn-primary:hover:not(:disabled) { background: var(--c-primary-hover); }
.btn-secondary { background: transparent; color: var(--c-text); border-color: var(--c-border-strong); }
.btn-secondary:hover:not(:disabled) { background: var(--c-surface-2); }
.btn-ghost { background: transparent; color: var(--c-text-muted); }
.btn-ghost:hover:not(:disabled) { background: var(--c-surface-2); color: var(--c-text); }
.btn-danger { background: var(--c-danger-bg); color: var(--c-danger-text); border-color: var(--c-danger); }
.btn-danger:hover:not(:disabled) { background: var(--c-danger); color: var(--c-primary-text); }
.btn-sm { padding: var(--space-xs) var(--space-sm); font-size: var(--fs-xs); }

/* legacy .action-button class used by the existing /me/security page —
   kept until Phase D rewrites those pages. */
.action-button {
    display: inline-flex; align-items: center; padding: var(--space-sm) var(--space-md);
    background: var(--c-primary); color: var(--c-primary-text);
    border: none; border-radius: var(--radius-sm);
    font-size: var(--fs-sm); font-weight: 600; cursor: pointer;
}
.action-button:hover:not(:disabled) { background: var(--c-primary-hover); }
.action-button:disabled { opacity: 0.5; cursor: not-allowed; }
.action-link { color: var(--c-primary); text-decoration: underline; }
.action-link:hover { color: var(--c-primary-hover); }

/* ──────────────────────────────────────────────────────────────────
   10. Inline result panels (test-send, save, etc.)
   ────────────────────────────────────────────────────────────────── */

.inline-result {
    margin-top: var(--space-md);
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--radius-sm);
    border: 1px solid var(--c-border);
    font-size: var(--fs-sm);
    background: var(--c-surface-2);
}
.inline-result.success { border-color: var(--c-up); color: var(--c-up); background: var(--c-up-bg); }
.inline-result.error { border-color: var(--c-down); color: var(--c-down); background: var(--c-down-bg); }
.inline-result.warn { border-color: var(--c-degraded); color: var(--c-degraded); background: var(--c-degraded-bg); }
.inline-result.info { border-color: var(--c-info); color: var(--c-info); background: var(--c-info-bg); }
.inline-result[hidden] { display: none; }

/* ──────────────────────────────────────────────────────────────────
   11. Tabs
   ────────────────────────────────────────────────────────────────── */

.tabs {
    display: flex;
    gap: var(--space-xs);
    border-bottom: 1px solid var(--c-border);
    margin-bottom: var(--space-lg);
    flex-wrap: wrap;
}
.tabs a, .tabs button {
    padding: var(--space-sm) var(--space-md);
    font-size: var(--fs-sm);
    color: var(--c-text-muted);
    text-decoration: none;
    border: none;
    background: transparent;
    border-bottom: 2px solid transparent;
    margin-bottom: -1px;
    cursor: pointer;
    font-family: inherit;
    font-weight: 500;
}
.tabs a:hover, .tabs button:hover { color: var(--c-text); }
.tabs a[aria-current="page"], .tabs button[aria-selected="true"] {
    color: var(--c-primary);
    border-bottom-color: var(--c-primary);
    font-weight: 600;
}

/* ──────────────────────────────────────────────────────────────────
   12. Tables
   ────────────────────────────────────────────────────────────────── */

table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--fs-sm);
}
th, td {
    padding: var(--space-sm) var(--space-md);
    text-align: left;
    border-bottom: 1px solid var(--c-border);
}
th {
    font-weight: 600;
    color: var(--c-text-muted);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    background: var(--c-surface-2);
}
tbody tr:hover { background: var(--c-surface-2); }

/* ──────────────────────────────────────────────────────────────────
   13. Forms
   ────────────────────────────────────────────────────────────────── */

label {
    display: block;
    font-size: var(--fs-sm);
    color: var(--c-text-muted);
    margin-bottom: var(--space-xs);
    font-weight: 500;
}
input[type="text"], input[type="email"], input[type="number"], input[type="url"],
input[type="search"], input[type="password"], select, textarea {
    width: 100%;
    padding: var(--space-sm) var(--space-md);
    background: var(--c-bg);
    color: var(--c-text);
    border: 1px solid var(--c-border-strong);
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
    font-family: inherit;
}
input:focus-visible, select:focus-visible, textarea:focus-visible {
    border-color: var(--c-focus);
}
.field { margin-bottom: var(--space-md); }
.field-help { font-size: var(--fs-xs); color: var(--c-text-quiet); margin-top: var(--space-xs); }
.form-row { display: flex; gap: var(--space-sm); align-items: center; flex-wrap: wrap; }
.form-actions { display: flex; gap: var(--space-sm); margin-top: var(--space-lg); }

/* ──────────────────────────────────────────────────────────────────
   14. Definition list (used in info-grid)
   ────────────────────────────────────────────────────────────────── */

.info-grid {
    display: grid;
    grid-template-columns: minmax(120px, max-content) 1fr;
    gap: var(--space-xs) var(--space-md);
    font-size: var(--fs-sm);
}
.info-grid dt { color: var(--c-text-muted); }
.info-grid dd { color: var(--c-text); }

/* ──────────────────────────────────────────────────────────────────
   15. Footer
   ────────────────────────────────────────────────────────────────── */

footer[role="contentinfo"] {
    padding: var(--space-lg);
    text-align: center;
    font-size: var(--fs-xs);
    color: var(--c-text-quiet);
    border-top: 1px solid var(--c-border);
}

/* ──────────────────────────────────────────────────────────────────
   16. Visually-hidden (for screen-reader-only labels)
   ────────────────────────────────────────────────────────────────── */

.sr-only {
    position: absolute;
    width: 1px; height: 1px; padding: 0;
    overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0;
}

/* ──────────────────────────────────────────────────────────────────
   17. Responsive (mobile-first refinements)
   ────────────────────────────────────────────────────────────────── */

@media (max-width: 768px) {
    .header-inner { gap: var(--space-sm); }
    nav[aria-label="Main navigation"] { width: 100%; }
    .nav-group { width: 100%; }
    .nav-group ul { flex-wrap: wrap; }
    .metric-grid, .summary-grid { grid-template-columns: repeat(2, 1fr); }
    table { display: block; overflow-x: auto; -webkit-overflow-scrolling: touch; }
    .info-grid { grid-template-columns: 1fr; gap: var(--space-2xs); }
    .info-grid dt { margin-top: var(--space-sm); }
    .form-actions { position: sticky; bottom: 0; background: var(--c-bg); padding: var(--space-sm) 0; }
}

/* ──────────────────────────────────────────────────────────────────
   18. Reduced motion
   ────────────────────────────────────────────────────────────────── */

@media (prefers-reduced-motion: reduce) {
    *, *::before, *::after { transition: none !important; animation: none !important; }
}
"#;
