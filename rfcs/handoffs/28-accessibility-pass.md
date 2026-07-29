# 28 — Accessibility pass across the surface

**Milestone** M4 · **Satisfies** NFR-A11Y-01…13, NFR-I18N-04
**Branch** `feat/28-a11y-pass` · **Depends on** subjects 26 and 27 complete
**Governing artifact** — **RFC 0011** (DEC-015)

## Why this is a separate subject

Per-screen assertions catch per-screen regressions. They do not catch
what only appears across the whole surface: a tab order that breaks at a
screen boundary, a landmark present everywhere except one route, motion
that survives in one component.

ABDD is a baseline property, not a polish step — but a whole-surface
check still has to happen once the surface exists.

## Build

- **Keyboard traversal end to end**, using native element behaviour only.
  No scripted focus management (NFR-A11Y-07)
- **Landmarks on every route** — banner, navigation, main, contentinfo
  (NFR-A11Y-04)
- **Motion suppressed** under `prefers-reduced-motion` (NFR-A11Y-08)
- **Contrast pinning green** in both themes **and** both languages
- **Primary form actions reachable** on narrow viewports without
  scrolling past a long form (NFR-A11Y-13)
- **Focus visibly indicated** on all interactive elements (NFR-A11Y-06)
- **Readable without CSS** on every route (NFR-A11Y-09)

## Verify

| # | Test | Type |
|---|---|---|
| T-133 | Keyboard traversal across all sixteen screens uses native behaviour only | guard |
| T-134 | Every route exposes all four landmarks | guard |
| T-135 | Motion is suppressed under `prefers-reduced-motion` | guard |
| T-136 | Contrast pins green in both themes and both languages | **guard — critical** |
| T-137 | Primary form actions stay reachable on a narrow viewport | guard |
| T-138 | Every route is readable with CSS unavailable | guard |

## Done

- All six pass across all sixteen screens
- `docs/src/external-design.md` §4.2 matches what shipped, route for route
- RFC 0011 → `rfcs/done/`, `Status: Implemented (0.40.0)`
- `docs/src/requirements.md`: NFR-A11Y-01…13 re-verified rather than assumed

**→ Cut v0.40.0 (M4) after subjects 25–28 are merged.**

## Escalate

A pinned contrast value changed in a diff → the control has been
inverted. This is not a review comment.
