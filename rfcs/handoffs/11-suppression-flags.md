# 11 — Suppression windows honour their own flags

**Milestone** M2 · **Closes** G-07 · **Satisfies** FR-SUP-07, FR-SUP-08, FR-SUP-13
**Implements** DEC-013 · **Branch** `fix/11-suppression-flags` · **Depends on** subject 10
**Governing artifact** — Gap **G-07** (§11) · **DEC-013** decides the two-flag model

## The defect

Two code paths consume windows, and each ignores a flag:

| Path | Filters on | Should filter on |
|---|---|---|
| `is_under_maintenance` — notification | `is_active` only | `is_active` **and** `suppress_notify` |
| `list_in_window` — SLA exclusion | **nothing** | `is_active` **and** `exclude_from_sla` |

So a window explicitly marked as non-suppressing **still suppresses**,
and a deactivated window **still moves the SLA figure**. The interface
offers a control that does not do what it says.

## Build — DEC-013, two flags

**Migration `sql/0006`, part one:**

```sql
ALTER TABLE maintenance_windows
  ADD COLUMN exclude_from_sla INTEGER NOT NULL DEFAULT 1;
```

Existing rows default to 1, preserving today's *intended* behaviour.

**Queries** — `crates/core/src/db/maintenance.rs`:

- `is_under_maintenance`: filter `is_active = 1 AND suppress_notify = 1`
- `list_in_window`: filter `is_active = 1 AND exclude_from_sla = 1`

**Interface (S-05)** — replace the single checkbox with three named
situations. Each states its own consequence, so the help card stops
having to explain a package deal:

| Choice | `suppress_notify` | `exclude_from_sla` | |
|---|---|---|---|
| Planned maintenance | 1 | 1 | default |
| Known external outage | 1 | 0 | downtime was real; do not forgive it |
| Expected noise | 0 | 1 | keep alerting; do not count it |

Links or radios, not script — the screen must work without JavaScript
(NFR-A11Y-10). Each window in the listing states both behaviours in
text, not colour alone (NFR-A11Y-03).

### Why two flags

Under one flag, `suppress_notify = false` produces a window that neither
silences nor excludes — an inert record nobody would create. More
importantly, an operator wanting to stop being paged for a third-party
outage would be forced to also forgive the downtime, **inflating the
number the product exists to report.** A missed page is an
inconvenience; an overstated availability figure is a false claim.

## Verify

| # | Test | Type |
|---|---|---|
| T-52 | A window with `suppress_notify = 0` does **not** silence notifications | **must fail first** |
| T-53 | A window with `is_active = 0` does **not** affect the SLA figure | **must fail first** |
| T-54 | A window with `exclude_from_sla = 0` silences alerts but leaves the SLA figure unchanged | **must fail first** |
| T-55 | A window with both flags set silences **and** excludes | guard |
| T-56 | The form offers three situations and works with scripting disabled | **must fail first** |
| T-57 | Listings state both behaviours in text, not colour alone | **must fail first** |

**T-54 is the test that proves DEC-013 was worth taking** — the known
third-party outage that should page nobody and be forgiven by nothing.

## Done

- All six tests pass; five baseline failures captured
- `docs/src/external-design.md` S-05 records the three-situation control
- `docs/src/requirements.md`: FR-SUP-07, FR-SUP-08, FR-SUP-13 →
  `Implemented`, G-07 struck
