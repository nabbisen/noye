# RFC 0001: Manual theme toggle (light / dark / system)

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-05-04
**Related ROADMAP item**: "Manual theme toggle (light / dark / system)" under `## UI / theme`
**Estimated size**: medium
**Implementation target**: post-0.27.x

---

## Summary

Allow a signed-in user to override their browser's
`prefers-color-scheme` for the duration of their Noye session — picking
light, dark, or "system" (= follow the OS) explicitly. The choice
persists across reloads via a cookie and applies before the first paint
so there is no flash of mismatched theme. The implementation is layered
on top of the Phase A design tokens and adds no new colour values.

## Background

Phase A (0.23.0) introduced a token-based design system with separate
light and dark token blocks. Theme selection has been entirely passive
since then — `@media (prefers-color-scheme: light)` overrides the dark
default, so the OS setting wins. This is fine for the common case but
fails one real workflow: an admin running a dark-themed OS who wants
Noye in light mode for a daytime briefing (or vice versa) cannot do
anything about it without changing OS settings.

The token infrastructure for this is already in place; what is missing
is a way to *select* a token block at runtime.

## Design

### User-visible surface

A three-state toggle in the user-info chip in the top-right corner of
every page chrome. The states cycle on click:

| State | `data-theme` attribute on `<html>` | Effective stylesheet |
|---|---|---|
| `system` (default) | (absent) | `@media (prefers-color-scheme)` decides |
| `light` | `data-theme="light"` | Light token block forced regardless of OS |
| `dark` | `data-theme="dark"` | Dark token block forced regardless of OS |

The button label and `aria-label` reflect the *next* state on click,
e.g. "Theme: system (click for light)".

### Persistence

A cookie named `noye_theme` carries the value `system` / `light` /
`dark`:

| Attribute | Value | Reason |
|---|---|---|
| `Path` | `/` | Apply across the whole UI |
| `Max-Age` | `31536000` (1 year) | Effectively persistent for the user |
| `SameSite` | `Lax` | Same as `noye_session`; OIDC top-level callback compatibility |
| `Secure` | set when `NOYE_ENV != development` | Production must be HTTPS-only |
| `HttpOnly` | **unset** | The pre-paint inline script needs to read it (see below) |

The cookie is written only by the server-side route below; the client
never writes it directly. This keeps audit-log clarity (no client-side
tampering of preferences makes the cookie value stable in security
reasoning).

### Server-side write path

A new endpoint `POST /me/theme` accepts `theme=system|light|dark` as a
form field, validates it against the closed set, sets the cookie, and
redirects back to `Referer` (sanitized via the existing `safe_redirect`
helper). The endpoint requires a valid session and the existing
Synchronizer Token Pattern CSRF token (`X-CSRF-Token` header or
`csrf_token` form field).

The button on the user chip submits this endpoint as a no-JS-required
`<form method="POST" action="/me/theme">` so the toggle works even
with JavaScript disabled.

### Pre-paint application

To avoid a flash of mismatched theme on page load, an inline `<script>`
in `<head>` (before any stylesheet link) reads the cookie and writes
the `data-theme` attribute on `<html>` synchronously:

```html
<script>
  (function () {
    var m = document.cookie.match(/(?:^|;\s*)noye_theme=(system|light|dark)/);
    if (m && (m[1] === 'light' || m[1] === 'dark')) {
      document.documentElement.setAttribute('data-theme', m[1]);
    }
  })();
</script>
```

This is the only part of the toggle that requires JavaScript. The
no-JS path still works (the form submit reloads the page, which then
has the correct cookie and renders correctly server-side because
`wrap()` reads the cookie too — see below).

### Server-side rendering parity

The `wrap()` helper in `gateway::ui::layout` reads the `noye_theme`
cookie and emits `<html data-theme="...">` directly when the value is
`light` or `dark`. This means:

- A no-JS user submits `POST /me/theme`, the page redirects, the next
  render carries the right `data-theme` attribute from the start.
- A JS-enabled user gets the snappier inline-script path that prevents
  any flash even on the very first request after toggling.

### CSS extension

Add `[data-theme="light"]` and `[data-theme="dark"]` selectors to
`gateway::ui::layout::style` mirroring the existing
`@media (prefers-color-scheme)` blocks. Specificity is low so component
CSS continues to work unchanged.

## Requirements

- The toggle MUST persist across page reloads and browser restarts up
  to the cookie expiry.
- The toggle MUST work without client-side JavaScript (form submit
  fallback).
- The first paint after a reload MUST NOT show the wrong theme even
  briefly (no flash) when JavaScript is enabled.
- A page render in either theme MUST continue to satisfy the WCAG AA
  thresholds pinned by
  `gateway::ui::layout::contrast::tests::critical_pairs_meet_aa`.
- The new `POST /me/theme` endpoint MUST require a valid session and
  pass CSRF validation; an unauthenticated POST MUST return `401`.
- Setting `noye_theme` to any value outside the closed set
  `{system, light, dark}` MUST be rejected with `400`.
- A pure helper `parse_theme_cookie(value: &str) -> Theme` MUST be
  unit-testable on the host target.
- The toggle button MUST be keyboard-operable (tab-focusable, activated
  by Enter / Space) and carry an `aria-label` that announces both the
  current and the next state to screen readers.

## Test plan

### Host unit tests (target: `gateway::ui::layout::theme`)

- `parse_theme_cookie`: returns `Theme::Light` for `"light"`,
  `Theme::Dark` for `"dark"`, `Theme::System` for `"system"` and for
  any unknown / empty / mixed-case value.
- `next_theme_in_cycle`: `System → Light → Dark → System`.
- `wrap_emits_data_theme_attribute_when_set`: `wrap()` with cookie
  value `"light"` produces `<html ... data-theme="light"`; with
  `"system"` or absent, the attribute is omitted.
- `aria_label_announces_next_state`: the button's `aria-label` mentions
  the next state in the cycle.

### Host unit tests (target: `gateway::handlers::theme`)

- `post_theme_rejects_unauthenticated_caller_with_401`.
- `post_theme_rejects_missing_csrf_token_with_403`.
- `post_theme_rejects_value_outside_closed_set_with_400`.
- `post_theme_writes_cookie_with_path_root_max_age_one_year`.
- `post_theme_redirects_to_referer_via_safe_redirect`.
- In production env, the cookie MUST carry `Secure`; in development it
  MUST not.

### Contrast regression

`critical_pairs_meet_aa` continues to pass with no changes — the
forced themes use the same token values as the `prefers-color-scheme`
branches, so this test exercises the same pairs.

### Manual smoke

Cycle the toggle in three browsers (one with JS disabled) and confirm
no flash, correct persistence across reloads, correct behaviour when
the OS toggles its theme between visits with `system` selected.

## Security considerations

- **Cookie tampering.** The cookie is not authenticated, so a user can
  edit it. The validation closed-set on read means a tampered value
  collapses to `system`. There is nothing privileged behind a theme
  preference, so authenticity is not required.
- **CSRF.** The state mutation is wrapped by the existing Synchronizer
  Token Pattern. The endpoint follows the pattern of every other
  mutating endpoint and does not bypass it.
- **Open redirect.** The redirect-back uses the existing
  `safe_redirect::sanitize_return_to` helper; no new redirect path is
  introduced.
- **CSP.** The pre-paint inline `<script>` is inline. The current CSP
  permits `'unsafe-inline'` for scripts; this RFC does not change
  that. If we tighten CSP later (RFC TBD), this script should be
  emitted with a per-response nonce.

## Out of scope

- A "high-contrast" preset (separate RFC).
- Per-page or per-section theme overrides.
- Time-of-day-based automatic theme switching.
- Custom palette injection.

## Migration / rollout notes

No migration. The cookie is absent on first request after deploy and
the system path is the default, so existing users see no change unless
they actively toggle.
