# 17 — States the system cannot produce are not representable

**Milestone** M2 · **Closes** G-17, G-28 · **Satisfies** FR-INC-10
**Implements** DEC-014 · **Branch** `fix/17-unreachable-states` · **Depends on** subject 16
**Governing artifact** — Gaps **G-17**, **G-28** (§11) · **DEC-014** decides removal over implementation

## The defects

Two tables, same defect: a CHECK constraint admits a value nothing
produces.

**G-17 — incidents.** `sql/0001_initial.sql:81` permits
`'acknowledged'`. No code path produces it, no query reads it, no
interface offers it. The glossary is explicit that incident states are
Open and Resolved.

**G-28 — target states.** `:49` permits `degraded` and `maintenance`.
`decide_transition` produces only up/down, and `db/states.rs` writes only
what it produces. But `crates/core/src/db/targets.rs:12-13` **counts
them** for the dashboard status breakdown — so two of that section's four
categories are structurally always zero, and given FR-UI-08 omits
all-zero sections, they are dead surface with live query code behind
them.

## Build

Per **DEC-014**, acknowledgement is removed rather than implemented. The
full implementation design is recorded in
[RFC 0010](../proposed/010-incident-acknowledgement.md) should
the decision ever be revisited.

1. Remove `'acknowledged'` from the `incidents.status` constraint.
2. Remove `degraded` and `maintenance` from the
   `target_states.current_status` constraint.
3. Remove both counts from the summary query in `db/targets.rs`.
4. Drop both categories from the dashboard breakdown.

Steps 3 and 4 matter as much as 1 and 2: leaving query code computing
counts that cannot be non-zero is how the constraint value looked
meaningful in the first place.

Land in the same migration as subject 18, since reopening a CHECK
constraint afterwards costs a second table rebuild.

## Verify

| # | Test | Type |
|---|---|---|
| T-83 | `incidents.status = 'acknowledged'` is rejected | **must fail first** |
| T-84 | `target_states.current_status = 'degraded'` is rejected | **must fail first** |
| T-85 | …and `'maintenance'` is rejected | **must fail first** |
| T-86 | The dashboard breakdown renders no category that cannot be non-zero | **must fail first** |

## Done

- All four tests pass; four baseline failures captured
- RFC 0010 → `rfcs/archive/`, `Status: Withdrawn — acknowledgement removed per DEC-014`
- `docs/src/requirements.md`: FR-INC-10 → `Implemented`; G-17, G-28 struck
