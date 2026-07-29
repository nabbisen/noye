# 27 — Three new screens

**Milestone** M4 · **Implements** DEC-015 · **Branch** one per screen
**Depends on** subject 25 · Independent of subject 26 — may interleave
**Governing artifact** — **RFC 0011** (DEC-015)

## Scope

| Screen | Answers |
|---|---|
| **Incident detail** | Today an incident is a table row. Diagnosis needs cause, timeline, the target's recent results, and the resolution path in one place |
| **Channel detail** | Specified as S-07 and partially shipped. Shows what a channel is attached to **before** you delete it |
| **Target statistics** | `/stats/:id` exists; this adds the trend context that answers "is this getting worse" |

Each needs a Core endpoint, a Gateway route, the UI, and tests.

## Build

**Add each to `docs/src/external-design.md` §4.2 before implementing**,
per that document's §14. A route that exists in code and not in the
design document is the failure mode the original review found four
instances of.

Write host-target render tests **before** styling — the UI layer is pure
functions returning `String`, so there is no reason to wait for CSS to
start asserting.

If subject 29's delivery records land first, surface them on incident
detail. That is the screen where "was this notified?" is actually asked.

## What must not regress

The same nine per-screen assertions as subject 26 apply here, including
the member-markup assertion. A new screen is not exempt because it is
new — it is more exposed, because it has no existing tests to inherit.

## Verify

| # | Test | Type |
|---|---|---|
| T-130 | Each new route appears in `external-design.md` §4.2 | **must fail first** |
| T-131 | Every route that exists is in §4.2, and every route in §4.2 exists | **must fail first** |
| T-132.<screen> | The nine per-screen assertions, per screen | guard |

**T-131 catches the drift the original review found four instances of** —
documentation describing interfaces the system does not have, and vice
versa. Make it mechanical so it cannot rot again.

## Done

- Three screens shipped, each in §4.2
- T-131 mechanical and green
- Per-screen coverage matrix extended to sixteen screens

## Escalate

A screen needing a Core endpoint that does not exist → that is an
external-design change, not an implementation detail. Raise it.
