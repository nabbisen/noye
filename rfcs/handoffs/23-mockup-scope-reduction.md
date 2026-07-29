# 23 — Reduce the mockup to the decided scope

**Milestone** M3 · **Implements** DEC-015 · **Branch** `feat/23-mockup-scope`
**Depends on** subject 21
**Governing artifact** — **DEC-015** (screen scope, from RFC 0011)

## Why this exists

The mockup is M4's design reference. It currently contains concepts the
project has decided against, and leaving them in means someone rebuilds
one by accident.

This subject makes it a truthful reference rather than a source of
rejected ideas.

## Build

### Remove

| From the mockup | Decided against by |
|---|---|
| Four-role capability ladder | DEC-009 — two roles |
| Tenant-administrator vocabulary, tenant-scoped views | DEC-008 — single tenant |
| On-call status, on-shift landing, quiet hours | §2.4 non-goal |
| Operations console, system console | DEC-015 |
| API tokens, notification preferences, trends, activity log | Deferred to `ROADMAP.md` — **listed there, not deleted** |

Re-express the remainder as two roles with owner scoping.

### Keep

The thirteen production screens plus the three DEC-015 accepts: incident
detail, channel detail, target statistics.

### Note

The deferred set is not rejected. Confirm each has a `ROADMAP.md` entry
with the reason before removing it from the mockup — P-1 requires "useful
but out of scope" to have a destination.

## Verify

| # | Test | Type |
|---|---|---|
| T-110 | The mockup renders only screens in the decided scope | **must fail first** |
| T-111 | Its role toggle offers `admin` and `member` only | **must fail first** |
| T-112 | Every deferred screen has a `ROADMAP.md` entry with a reason | guard |

## Done

- All three tests pass
- No route remains that subject 26 or 27 is not going to build

## Escalate

A screen you were told to remove looks load-bearing to another screen →
report before removing.
