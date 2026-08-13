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
what it produces. But `crates/core/src/db/targets.rs:59-60` **counts
them** for the dashboard status breakdown — so two of that section's four
categories are structurally always zero, and given FR-UI-08 omits
all-zero sections, they are dead surface with live query code behind
them.

> **Line numbers corrected 2026-08-13** — M2b's `TARGET_COLUMNS` moved
> these from `:12-13` to `:59-60`.

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

> **⚠️ `degraded` and `maintenance` each name two different things, and
> only one of each is dead.** Removing these strings from the UI
> wholesale breaks two live features — one of them shipped by M2b four
> days ago. Work from this list, not from a search for the words.

| Remove | Keep — and why |
|---|---|
| `db/targets.rs:59,60` — the two `SUM(CASE WHEN … )` counts | `BadgeKind::Degraded` / `BadgeKind::Maintenance` and their CSS classes and labels (`ui/layout/components.rs:87,98,113,114,145,146`) |
| `db/targets.rs:72,92` — the `degraded_count`/`maint_count` struct field and its mapping | `ui/dashboard.rs:51` — open incidents map to `MetricTone::Degraded`. Nothing to do with target status |
| `db/targets.rs:82` — the two fields in the test fixture | **`ui/maintenance.rs:201-202` — active suppression windows render `status_badge("maintenance")`.** This is M2b's listing table, and it reuses the badge's tone deliberately (see its own comment) |
| `ui/dashboard.rs:154,160,165` — the two `<dt>`/`<dd>` pairs, and their two terms in `interesting` | |
| `ui/layout/components.rs:117,149` and its test at `:386` — the `'acknowledged'` badge and label mappings | |

> **`ui/dashboard.rs:154` is the one to get right.** `interesting =
> summary.degraded + summary.maintenance + summary.unknown +
> summary.disabled` is FR-UI-08's all-zero omission test. Drop the two
> terms; keep the line and the behaviour.

## Verify

| # | Test | Type |
|---|---|---|
| T-83 | `incidents.status = 'acknowledged'` is rejected | **must fail first** |
| T-84 | `target_states.current_status = 'degraded'` is rejected | **must fail first** |
| T-85 | …and `'maintenance'` is rejected | **must fail first** |
| T-86 | The dashboard breakdown renders no category that cannot be non-zero | **must fail first** |
| T-86a | An active suppression window still renders its `maintenance` badge, and an open incident still renders the `degraded` tone | guard |

**T-83–T-85 go in `scripts/check-migrations.sh`** — they are CHECK
refusals against a fresh `sqlite3` database, the same shape as its
existing T-27, and **G-37** means `noye-core` has nowhere else to put
them. T-86 and T-86a are host tests in `ui/dashboard.rs` and
`ui/maintenance.rs`.

**T-86a is the guard for the collision above.** It is cheap and it is the
only thing standing between a correct removal and a silent regression of
M2b's listing table.

## Done

- All five tests pass; four baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` — the wasm suites, not just `cargo check`
- RFC 0010 → `rfcs/archive/`, `Status: Withdrawn — acknowledgement removed per DEC-014`
- `docs/src/requirements.md`: FR-INC-10 → `Implemented`; G-17, G-28 struck
