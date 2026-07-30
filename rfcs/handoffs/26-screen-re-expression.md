# 26 — Re-express the thirteen existing screens

**Milestone** M4 · **Satisfies** FR-UI-01…20, NFR-A11Y-01…13, NFR-I18N-01
**Branch** one per screen, `feat/26-<screen>` · **Depends on** subject 25
**Governing artifact** — **RFC 0011** (DEC-015)

## Scope

`/` · `/targets` · `/targets/:id` · `/incidents` · `/maintenance` ·
`/channels` · `/channels/:id` · `/stats` · `/stats/:id` · `/audit` ·
`/me/security` · `/settings` · `/admin/migration`

`/targets` is already done — subject 21's spike. Twelve remain.

**One branch per screen.** A screen is small enough to review properly;
twelve are not.

## Order

**Sequence from `.git-exclude/evidence/21-spike-report.md`.** If the spike
found the component layer covered most of the target list's needs, the
other table-shaped screens follow cheaply and go next. If per-screen
judgement dominated, take the highest-traffic screens first so the
benefit lands early.

## Build

Each screen keeps its route, its `?tab=` / `?window=` contract, and its
existing tests.

**Convert its strings as you go**, using subject 22's mechanism. Doing
this per screen rather than in a sweep afterwards is the difference
between one pass and two.

## What must not regress — assert per screen

These are FR-UI and NFR-A11Y requirements with existing tests. During
re-expression **the tests are the contract**:

| Assertion | Requirement |
|---|---|
| Exactly one `<main>` and one visible `<h1>` | FR-UI-04 |
| Skip link is the first focusable element | NFR-A11Y-05 |
| **Member-rendered markup contains no admin control markup** | FR-RBAC-05 |
| Renders with scripting disabled | NFR-A11Y-10 |
| Readable with CSS unavailable | NFR-A11Y-09 |
| Every form control has a programmatic label | NFR-A11Y-12 |
| Every instant uses `<time datetime="…">` | FR-UI-14 |
| No `alert`, `prompt` or `confirm` | FR-UI-15 |
| Section state in the URL; unknown values fall back | FR-UI-06 |

### The one that will bite

**FR-RBAC-05.** The mockup had no real authorization — its role chip was
a demonstration toggle, and its own documentation says so. A screen
rebuilt from it is exactly where "absent for members" quietly becomes
"hidden with CSS", and CSS hiding is not authorization.

**Every screen needs its own member-markup assertion**, asserting on the
*source*, not on visibility. Per screen, not once for the application.

## Verify

Number tests `T-121.<screen>` through `T-129.<screen>` so coverage is
visible at a glance. Produce a **per-screen coverage matrix** — thirteen
screens against nine assertions. A gap in that grid is the only way this
subject loses a guarantee quietly, and the grid is what makes it visible.

No test may assert on translated display text.

## Done

- Every screen renders from the refreshed design
- The coverage matrix is complete with no gaps
- `docs/src/requirements.md`: NFR-I18N-01 → `Implemented`

## Escalate

An existing FR-UI or NFR-A11Y test failing after re-expression → the
re-expression is wrong, not the test. A screen without a member-markup
assertion is not done.
