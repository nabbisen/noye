# 15 — One open incident per target, enforced by the database

**Milestone** M2 · **Closes** G-11 · **Satisfies** FR-INC-03, DR-INT-05
**Branch** `fix/15-one-open-incident` · **Depends on** subject 14
**Governing artifact** — Gap **G-11** (§11)

## The defect

"At most one open incident per target" is a property of application flow
only. Re-entrant scheduling, manual operations, or any future
concurrency can produce duplicates, and nothing would stop it.

## Build

```sql
CREATE UNIQUE INDEX idx_incident_one_open
    ON incidents(target_id) WHERE status = 'open';
```

SQLite supports partial unique indexes.

Resolve any pre-existing duplicates inside the migration and **record in
the migration comment what you found and what you did with them** —
duplicates would tell us the application flow has already been
re-entered, which is worth knowing.

Per DEC-014 the index covers `open` alone; `acknowledged` is being
removed in subject 17.

## Verify

| # | Test | Type |
|---|---|---|
| T-76 | A second `open` incident for the same target is rejected **by the database** | **must fail first** |
| T-77 | Resolving the first allows a new one | guard |
| T-78 | Pre-existing duplicates are resolved and reported | guard |

**T-76 must bypass the API.** The point is that the *database* refuses
it. Insert directly; going through the application path proves only that
the existing flow control still works, which was never in doubt.

## Done

- All three tests pass; T-76's baseline failure captured
- Any duplicates found are reported before the migration resolves them
- `docs/src/requirements.md`: FR-INC-03, DR-INT-05 → `Implemented`, G-11 struck

## Escalate

Pre-existing duplicates found → report the count before resolving.
