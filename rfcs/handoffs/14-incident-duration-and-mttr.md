# 14 — Automatic resolution records a duration

**Milestone** M2 · **Closes** G-10 · **Satisfies** FR-INC-08
**Branch** `fix/14-incident-duration` · **Depends on** subject 13
**Governing artifact** — Gap **G-10** (§11)

## The defect

`crates/core/src/db/incidents.rs:26-27` sets `duration_sec` on manual
resolution. Line 44 — automatic resolution — does not. And
`crates/core/src/stats.rs` builds MTTR with
`filter_map(|i| i.duration_sec)`, so automatically-resolved incidents,
the overwhelming majority, contribute nothing.

The displayed MTTR is not merely incomplete. It is computed over an
unrepresentative minority and presented as if it were the whole picture —
**misleading rather than missing.**

## Build

1. Compute and store `duration_sec` on the automatic path exactly as the
   manual path does.
2. In `stats.rs`, derive duration from `resolved_at − opened_at` when the
   column is null, so rows written before this fix are not permanently
   excluded from reporting.

## Verify

| # | Test | Type |
|---|---|---|
| T-73 | An auto-resolved incident contributes to MTTR | **must fail first** |
| T-74 | A window containing only auto-resolved incidents returns an MTTR value, not none | **must fail first** |
| T-75 | A pre-existing row with a null `duration_sec` still contributes | **must fail first** |

## Done

- All three tests pass; three baseline failures captured
- `docs/src/requirements.md`: FR-INC-08 → `Implemented`, G-10 struck
