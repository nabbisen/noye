# RFC 0005: High-contrast theme preset

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-05-04
**Related ROADMAP item**: "High-contrast mode preset" under `## UI / theme`
**Estimated size**: small
**Implementation target**: post-RFC 0001

---

## Summary

Add an optional `high-contrast` theme preset selectable through the
manual theme toggle from RFC 0001. The preset overrides the colour
tokens with values that meet WCAG AAA (7:1 body, 4.5:1 large), pinned
by the existing contrast-check test suite. Layout, typography, and
spacing tokens are unchanged.

## Background

WCAG AAA contrast is achievable on top of the Phase A token system with
a small set of token overrides. Operators in low-vision contexts
sometimes need this beyond the AA baseline that ships today. We don't
flip the whole theme to AAA by default because the maintenance burden
of pinning more colour pairs accumulates with each new component.

This RFC takes the cheapest possible shape: a fourth value of the
`noye_theme` cookie alongside `system` / `light` / `dark`, gated behind
the toggle.

## Design

### Dependency on RFC 0001

This RFC requires the manual theme toggle infrastructure from RFC 0001
to be in place. The cookie value, the pre-paint inline script, the
`POST /me/theme` endpoint, the toggle button, and the `data-theme`
attribute on `<html>` are all reused unchanged.

### Token preset

A new CSS block in `gateway::ui::layout::style`:

```css
[data-theme="high-contrast"] {
  --c-bg: #000000;
  --c-surface: #0a0a0a;
  --c-surface-2: #141414;
  --c-text: #ffffff;
  --c-text-muted: #e0e0e0;
  /* ... etc, only the colour tokens that need stronger contrast */
}
```

Numeric values above are illustrative; the exact values are picked so
that every pair tested by
`gateway::ui::layout::contrast::tests::critical_pairs_meet_aa` meets
the AAA threshold (7:1 for body, 4.5:1 for large text and UI
controls).

The status badges (`badge-up`, `badge-down`, etc.) keep their hue
identity (red / green / yellow remain recognisable) but with luminance
shifted to clear the AAA bar against the surface.

### Toggle integration

The toggle button cycle is extended:

| RFC 0001 cycle | RFC 0005 cycle |
|---|---|
| system → light → dark → system | system → light → dark → high-contrast → system |

The button's `aria-label` continues to announce the next state.

### Server-side render

`wrap()` in `gateway::ui::layout` already reads the cookie and emits
`data-theme="..."` for `light` / `dark`. With this RFC the value
`high-contrast` joins the closed set. Validation in
`POST /me/theme` likewise extends the closed set.

### Test extension

`gateway::ui::layout::contrast::tests` gains a `critical_pairs_meet_aaa`
test that runs the same 25 pairs against the AAA threshold under the
high-contrast theme. The existing AA test continues to run for the
other three themes. A new test must catch any future contributor who
adds a token without giving it an AAA-compliant override.

## Requirements

- Every contrast pair pinned by
  `gateway::ui::layout::contrast::tests::critical_pairs_meet_aa` MUST
  also meet WCAG AAA (7:1 body / 4.5:1 large) under the high-contrast
  preset, verified by a parallel `critical_pairs_meet_aaa` test.
- The status-badge identity (colour family per state) MUST be preserved
  — `badge-up` is recognisably "green," `badge-down` is recognisably
  "red," etc. Pure greyscale is not acceptable.
- Selecting `high-contrast` from the toggle MUST persist via the
  existing `noye_theme` cookie (RFC 0001) and apply pre-paint via the
  same inline script.
- The toggle MUST gracefully degrade when JS is disabled: the form
  submit path from RFC 0001 covers it without changes.
- No new colour tokens MUST be introduced; only overrides of the
  existing set under the new selector.

## Test plan

### Host unit tests (target: `gateway::ui::layout::contrast`)

- `critical_pairs_meet_aaa_under_high_contrast_theme` — the same 25
  pairs the AA test pins, evaluated against the AAA threshold.
- `status_badge_hue_identity_preserved_in_high_contrast` — a coarse
  hue check confirming red / green / amber / blue families haven't
  collapsed to greyscale.

### Host unit tests (target: `gateway::ui::layout::theme`)

- `parse_theme_cookie_accepts_high_contrast_value` (extending the
  RFC 0001 test suite).
- `next_theme_in_cycle_includes_high_contrast`.

### Manual / smoke

- Cycle through all four states in two browsers (one with JS
  disabled). Confirm visual differences for each state and that no
  page is unreadable.

## Security considerations

None unique to this RFC. Inherits the considerations from RFC 0001;
no new endpoints, no new cookies, no new trust boundaries.

## Out of scope

- A separate "low-contrast" preset.
- User-customizable palettes.
- Automatic high-contrast switching based on `prefers-contrast` media
  query — the OS doesn't expose this reliably yet, so we keep the
  selection explicit.

## Migration / rollout notes

- No migration. The cookie value-set silently grows by one allowed
  string; pre-existing cookies (`system`/`light`/`dark`) keep working.
- Cannot ship before RFC 0001.
