# 25 — Design tokens and component layer

**Milestone** M4 · **Satisfies** NFR-A11Y-01, 02, 03, NFR-MNT-03
**Implements** [RFC 0011](../proposed/011-interface-integration.md) (DEC-015)
**Branch** `feat/25-tokens-components` · **Depends on** M3 shipped
**Governing artifact** — **RFC 0011** (DEC-015)

**This subject gates every other M4 subject.** Land it first.

## The one thing to understand

**Re-expression, never merge.** Leptos, axum and tokio do not compile for
`wasm32-unknown-unknown`. The mockup cannot be linked, vendored or
incrementally migrated. DEC-003 abandoned Leptos deliberately, and 435
host tests exist because of it.

**What is adopted is the design** — token values, component semantics,
copy, interaction patterns. The code is rebuilt as pure functions
returning `String`.

If the shortest path ever looks like importing mockup code, the answer is
no, and the reason is that it will not compile for the deployment target.

## Build

### Tokens

Reconcile the mockup's `assets/style.css` token values into
`crates/gateway/src/ui/layout/style.rs`. Then re-pin all 25 contrast
pairs in `crates/gateway/src/ui/layout/contrast.rs` against the new
values, light and dark.

### ⛔ Stop and report

If a pair fails WCAG AA on the new tokens, **change the token, never the
pinned threshold.** The pinning test exists precisely so a token edit
cannot silently regress contrast; editing the pin to make it pass inverts
the control into a rubber stamp. Report it as a design question.

### Components

Port the mockup's component *semantics* into
`crates/gateway/src/ui/layout/components.rs`. Contracts each must keep —
these are requirements with existing tests, not styling preferences:

| Component | Contract |
|---|---|
| Status badge | Shape marker **and** text label **and** colour. Colour is never the sole signal (NFR-A11Y-03) |
| Navigation | Grouped by verb; active item marked with `aria-current` (NFR-A11Y-11) |
| Tabs | Links, not scripted toggles; active marked as current page |
| Inline result | Live region, announced without stealing focus |
| Timestamp | `<time datetime="…">` carrying the exact instant (FR-UI-14) |
| Destructive region | Separated, with confirmation; focus returns on cancel |

## Verify

| # | Test | Type |
|---|---|---|
| T-117 | All 25 contrast pairs pass AA in both themes, **pins unchanged** | **guard — critical** |
| T-118 | …and in both languages | **guard — critical** |
| T-119 | No raw colour value appears in a component style | guard |
| T-120 | Every token referenced by a component resolves | guard |

**T-117 is a trap detector.** If you see a pinned value change in a diff,
that is an escalation, not a review comment.

## Done

- All four tests pass
- No screen work has begun before this merged

## Escalate

Any pinned contrast value needing to change → requirements architect.
