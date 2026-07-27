# Accessibility (ABDD)

The Noye web UI is built on the principle of **Accessible by Default and by Design**. Accessibility is not retrofitted; it is the starting point for every UI decision.

## Concrete commitments

- **Semantic HTML.** Every page uses `<header>`, `<nav>`, `<main>`, `<footer>`, `<table>`, `<dl>` (and similar elements) where they accurately describe the content. There are no decorative `<div>` containers wrapping content that has a more specific semantic.
- **WAI-ARIA where it adds meaning.** `role`, `aria-label`, `aria-current`, `aria-disabled`, and `aria-live` are used to clarify intent for assistive technology — not to compensate for missing semantics.
- **Three-verb navigation grouping.** The main nav is organized into three labelled groups — Observe (Dashboard / Incidents / Stats), Operate (Targets / Channels / Maintenance), Verify (Audit / Settings / Migration; admin-only). Each `<ul>` has an `aria-labelledby` heading so screen readers announce the group's purpose. Members never see the Verify group.
- **Keyboard navigation.** A "Skip to main content" link is the first focusable element on every page. `:focus-visible` styles ensure the focus ring is always present and high-contrast. Tab order follows reading order; there is no JavaScript-managed focus that conflicts with browser defaults.
- **Color contrast — verified at compile time.** Both the dark and light themes meet WCAG 2.1 AA contrast ratios. The 25 critical foreground/background pairs (body text on each surface depth in both themes; every status-badge fg/bg pair; the primary button) are pinned in `gateway::ui::layout::contrast::tests::critical_pairs_meet_aa`. Editing a token below threshold breaks the unit test before deploy. Status colors are also paired with text labels rather than carried by color alone.
- **Token-based design system.** Color, spacing (8-pt grid), radius, and typography all live as CSS custom properties under `:root` (dark theme baseline) with light overrides under `prefers-color-scheme: light`. Component CSS references token names only; no hex codes leak into per-page styles. This makes consistency mechanical: a new "card" looks identical to every other card because they all reach for the same `--c-surface` and `--space-lg`.
- **HTML-first rendering.** All pages are rendered server-side as plain HTML. CSS and JavaScript enhance the experience but are never required to read or operate the system. Status badges still convey their meaning when CSS is disabled because they include readable text.
- **Reduced motion.** The CSS respects `prefers-reduced-motion: reduce` and disables transitions/animations for users who request it.
- **Theme respect.** `prefers-color-scheme` is honored automatically; there is no theme toggle that overrides the OS preference. A manual toggle is on the roadmap (see `ROADMAP.md`); today's behavior is intentionally OS-following.

## What we do not do

- We do not generate JavaScript-only flows. Every interactive feature has a non-JS fallback (e.g. forms still POST without client-side handlers).
- We do not lock keyboard focus inside dialogs or surface elements without exit paths.
- We do not rely on hover-only affordances; everything that can be activated by hover is also activatable by keyboard and tap.
- We do not include emoji or icon-only buttons without accessible names.

## Testing notes

Accessibility regressions are caught most reliably by:

1. Tabbing through every page from top to bottom and verifying that every interactive element receives a visible focus indicator.
2. Disabling CSS in the browser dev tools and verifying that all status information is still legible.
3. Using a screen reader (VoiceOver, NVDA) on the dashboard, target detail, and incidents pages to confirm landmark navigation works.
