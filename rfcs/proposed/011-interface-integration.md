# RFC 0011: Interface integration from the UI mockup

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-07-28
**Related ROADMAP item**: M3 design freeze, M4 interface integration
**Estimated size**: large
**Implementation target**: 0.30.0 (freeze), 0.40.0 (integration)

---

## Summary

A parallel UI line — `noye-mockup` v0.6.10 — describes itself as the
accepted production UI contract. It is built on Leptos 0.8 + axum 0.8 +
tokio, none of which runs on Cloudflare Workers, and it contains 23
pages against the shipped 13. This RFC defines what is adopted, what is
not, and **how** adoption happens: by re-expression, never by merge.

Without it, "integrate the mockup" is an instruction with no defined
scope, no defined mechanism, and a technology stack incompatible with
the deployment target.

## Background

The mockup was built to fix interaction design cheaply, before a backend
existed. It succeeded at that: the token system, the status-badge
semantics, the progressive-disclosure discipline and the scenario-shaped
navigation are genuinely better than what ships today.

It also drifted past the product in ways already resolved against it:

| Mockup carries | Resolved as |
|---|---|
| Four-role capability ladder | Rejected — two roles ([DEC-009](../../docs/src/decision-log.md)) |
| Tenant-administrator vocabulary | Rejected — single tenant per deployment (DEC-008) |
| On-call status and on-shift landing | Rejected — explicit non-goal, §2.4 |
| EN + JA i18n | Open — [RFC 0009](009-multilingual-interface.md) |
| Acknowledge / quick-resolve | Rejected — [RFC 0010](../archive/010-incident-acknowledgement.md) withdrawn per DEC-014 |

So integration is not "take the mockup". It is "take what survives the
decisions already made".

## The mechanism: re-expression, not merge

Non-negotiable, and the single most misread point:

- The shipped UI is **pure functions returning `String`**, chosen
  deliberately (DEC-003) for host-target testability and to drop the
  framework dependency. 435 host tests exist because of it.
- Leptos, axum and tokio do not compile for
  `wasm32-unknown-unknown`. The mockup cannot be linked, vendored, or
  incrementally migrated into the Workers deployment.
- Its next-step list calls for a SQLite repository layer, not D1.

**What is adopted is the design, not the code**: layout, token values,
component semantics, copy, interaction patterns, and the accessibility
decisions. Every accepted screen is rebuilt against the existing Gateway
UI layer.

Anyone proposing to adopt the mockup's stack instead is proposing to
abandon CON-03, DR-STO-01…03, DEC-001, and the two-Worker security
model. That is a different product and needs its own RFC.

## Scope: what is accepted

**Adopted wholesale — applies to all screens:**

- Design tokens, reconciled into `ui/layout/style.rs`, with the 25
  pinned contrast pairs re-verified against the new values
- Component semantics: badge (text + shape + colour, never colour
  alone), navigation grouping, page header, metric card, inline result
  region
- Progressive disclosure as a working rule: a quiet default view, detail
  behind explicit disclosure
- Copy and tone, where it is calmer or clearer than what ships

**Adopted as new screens — provisional, pending decision D-B:**

| Screen | Why it earns a route |
|---|---|
| Incident detail | Today an incident is a table row. Diagnosis needs its own surface: cause, timeline, the target's recent results, and the resolution path in one place |
| Channel detail | Already specified as S-07 and partially shipped; the mockup's version is materially better at showing what a channel is attached to before you delete it |
| Target statistics | `/stats/:id` exists as S-09; the mockup adds per-target trend context that answers "is this getting worse" |

**Deferred to `ROADMAP.md`, not rejected:** trends, activity log,
notification preferences, API tokens.

**Rejected:** on-call status, on-shift landing, quiet hours, operations
console, system console, tenant-scoped views. Each conflicts with a
decision already taken; re-proposing any of them means reopening
DEC-008 or DEC-009 with a fresh case.

## Design

### Order of work

1. **Tokens and contrast.** Everything else renders against them, and a
   contrast regression found late is expensive.
2. **Component layer.** Badge, navigation, header, card, result region.
3. **Existing 13 screens**, re-expressed one at a time. Each keeps its
   route, its `?tab=` / `?window=` contract, and its tests.
4. **New screens**, each needing a Core endpoint, a Gateway route, UI
   and tests.
5. **Accessibility pass** across the whole surface.

Steps 1 and 2 are prerequisites for everything after them; steps 3 and 4
are independent of each other.

### The estimate that is not yet trustworthy

No screen has been re-expressed yet, so any per-screen figure is a
guess. **Phase 5 must re-express exactly one screen as a spike** and
re-baseline from the actual. Target list is the right candidate: it has
filters, a status badge, an empty state and role-conditional controls,
so it exercises most of the component layer.

### What must not regress

Every screen carries obligations that exist today and are easy to lose
in a rewrite:

- One `<main>`, one visible `<h1>`, skip link first in tab order
- Section state in the URL — `?tab=`, `?window=` — with unknown values
  falling back rather than erroring
- Works without JavaScript; works without CSS
- Admin-only controls **absent** from member markup, not hidden
- No browser dialog primitives anywhere
- `<time datetime="…">` for every instant

These are not polish. They are FR-UI and NFR-A11Y requirements with
existing tests, and the tests should be treated as the contract during
re-expression.

## Requirements

No requirement text changes. Adopted screens must satisfy the existing
FR-UI-01…20 and NFR-A11Y-01…13. New screens extend the S-nn catalogue in
`external-design.md`, which must be amended **before** implementation
per its §14.

## Test plan

- Every re-expressed screen keeps its existing tests passing, unmodified
  where the assertion is about structure rather than copy
- Contrast pinning passes on the new token values, light and dark
- Every route renders with scripting disabled
- Member markup contains no admin control markup, asserted per screen
- New screens have host-target render tests before they have styling

## Security considerations

Re-expression touches the layer that enforces FR-RBAC-05 — admin
controls absent from member-rendered HTML. A screen rebuilt from a
mockup that had no real authorization (its role chip was a demonstration
toggle) is exactly where that guarantee gets dropped by accident.
Per-screen member-markup assertions are the mitigation and are not
optional.

## Out of scope

- Any change to the Worker split, storage bindings, or authentication
- Adopting the mockup's role model, tenancy model, or on-call concepts
- The mockup's RFC corpus. Its 31 RFCs are the mockup's own history;
  importing them would collide with this project's numbering, which the
  lifecycle policy forbids renumbering to resolve. They are referenced,
  not merged
