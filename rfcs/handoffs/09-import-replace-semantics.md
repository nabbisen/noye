# 09 — Import replaces configuration, not history

**Milestone** M2 · **Closes** G-22 · **Satisfies** FR-MIG-08, FR-MIG-11, NFR-REL-03
**Branch** same as subject 08 · **Depends on** 08 (same branch)
**Governing artifact** — Gap **G-22** (§11)

## The defect

`upsert_target`, `upsert_channel` and `upsert_maintenance` all use
`INSERT OR REPLACE INTO …`.

Under SQLite, `REPLACE` resolves a conflict by **deleting** the existing
row and inserting a new one. With foreign keys enforced, that delete
fires every `ON DELETE CASCADE` declared against it
(`sql/0001_initial.sql:57,72,88,129-130`):

| Cascades from `targets` | Lost |
|---|---|
| `target_states` | the state row |
| `check_results` | all monitoring history |
| `incidents` | all incidents, open and resolved |
| `target_notifications` | every channel attachment |

Re-importing a configuration document with `on_conflict = replace` onto a
live deployment **silently destroys operational history — and reports
`Replaced`.**

The operator asked to update configuration and lost a month of results.

## Build

Replace all three with an explicit upsert that **updates in place**:

```sql
INSERT INTO targets (...) VALUES (...)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    ...
```

Dependent rows are untouched because no delete occurs.

### Do not

- Do not keep `INSERT OR REPLACE` anywhere in this module.
- Do not "solve" it by disabling foreign keys during import. The cascades
  are correct; triggering them from a *configuration update* is what is
  wrong.

## Verify

| # | Test | Type |
|---|---|---|
| T-42 | A `replace` import onto a target with existing check results, an open incident and two attached channels leaves **all three intact** | **must fail first** |
| T-43 | The `skip` policy still skips an existing target | guard |
| T-44 | The `fail` policy still reports collisions before writing anything | guard |
| T-45 | No `INSERT OR REPLACE` remains in `crates/core/src/db/migration.rs` | **must fail first** |

**T-42 is the most important test in M2.** Build the fixture explicitly:
create a target, insert check results, open an incident, attach two
channels, then import a document containing that target ID with
`on_conflict = replace`. Assert the counts of all four dependent tables
afterwards. Against today's code they go to zero while the import reports
success.

Record those four counts before and after, at baseline and after the fix.
They are the clearest evidence in M2 that a real defect existed.

## Done

- All four tests pass; two baseline failures captured
- `docs/src/external-design.md` §8.2 records that `replace` updates in place
- `docs/src/requirements.md`: FR-MIG-11 → `Implemented`, G-22 struck
