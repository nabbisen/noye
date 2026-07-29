# 02 — Retention deletes only what it archived

**Milestone** M0 · **Closes** G-20 · **Satisfies** DR-LIF-06, DR-LIF-07
**Branch** `fix/02-retention-scope` · **Depends on** nothing
**Governing artifact** — Gap **G-20** (§11)

## The defect

`crates/core/src/db/retention.rs`: `archive_old_records` selects
`… LIMIT 1000` and writes those rows to R2. The `DELETE` in
`run_cleanup` has **no limit**.

Any pass with more than 1000 eligible rows archives 1000 and deletes all
of them. At a few hundred targets on a one-minute cadence that is the
ordinary case for `check_results`, not an edge case. The excess is gone
permanently, unarchived, with no error.

NFR-REL-03 — "configuration and history MUST be recoverable after loss
of the primary database" — does not currently hold.

## Build

Restructure so deletion is scoped to the identities just archived:

```
for each retention policy:
    loop:
        batch := select up to N eligible records (id + row)
        if batch empty: break
        archive(batch)                  -- propagate failure with `?`
        delete records whose id ∈ batch.ids
    record last_cleanup_at
```

Also here:

- Replace the `format!`-interpolated SQL in `archive_old_records` with
  bound parameters. It is the only query in the codebase composing SQL by
  string concatenation.
- Replace the bare `_ => continue` with a visible diagnostic, so a
  retention policy naming an unhandled table surfaces rather than
  silently doing nothing.

### ⛔ Stop and report

**Verify D1's bound-parameter limit before choosing N.** A
`DELETE … WHERE id IN (?,?,…)` with 1000 placeholders may exceed it. If
so, lower N or sub-chunk within the batch — but the archived set and the
deleted set must still match exactly. If that property cannot be
satisfied within D1's limits, stop and report. **The specification is the
property, not the number.**

### Build step 4 — added 2026-07-28, after review of the delivered code

`run_cleanup` makes the archive conditional on `policy.archive_to_r2`
but the delete unconditional, so a policy with `archive_to_r2 = 0`
**deletes rows that were never archived** — recreating G-20's
consequence through configuration rather than through a bug.

DR-LIF-02 and DR-LIF-03 require archival before deletion for check
results and incidents specifically. For those classes the flag must not
be honourable.

**Treat `archive_to_r2 = 0` on a class that requires archival as a
configuration error: report and skip, do not delete.** Use the same
report-and-skip pattern already used for the unrecognised-table case.

### Do not

- Do not exempt `audit_logs` here — that is subject 04. After this
  subject audit rows are still deleted, and the tests must not assert
  otherwise yet.
- Do not accumulate deletions to the end of the pass. Deleting per batch
  is what makes a timed-out Worker invocation resumable.

## Verify

| # | Test | Type |
|---|---|---|
| T-04 | More than one batch eligible → count archived equals count deleted | **must fail first** |
| T-05 | Every deleted record appears in an archive object, matched by primary key | **must fail first** |
| T-06 | Forced archive failure → zero records deleted, failure surfaced | **must fail first** |
| T-07 | Aborted pass then successful pass → each record archived and deleted exactly once | **must fail first** |
| T-08 | A pass with no eligible records writes no object and deletes nothing | guard |
| T-09 | An unhandled table name produces a visible diagnostic | guard |
| T-09a | A policy with `archive_to_r2 = 0` on a class requiring archival deletes **nothing** and reports the misconfiguration | **must fail first** |

## Batch size

`RETENTION_BATCH_SIZE = 100` is deliberately used as **both** the
archive-select size and the delete-by-id chunk size, so one archived
batch maps to exactly one `DELETE`. Do not decouple them: archiving 1000
and deleting in ten chunks means a mid-chunk failure leaves rows archived
but not deleted, and the next pass archives them again — violating
DR-LIF-07's "without duplicating archived records".

The number itself is unverified against live D1 and is recorded as
**DEC-017** with re-verification criteria, closed by subject 36's
deployment rehearsal. Both failure directions are loud and fail-safe: too
high and D1 rejects the statement; too low and the pass exceeds an
invocation's subrequest budget and resumes on the next tick.

## Done

- All seven tests pass; five baseline failures captured
- No string-interpolated SQL remains in the module
- `docs/src/requirements.md`: DR-LIF-06, DR-LIF-07 → `Implemented`,
  G-20 struck

## Escalate

D1's parameter limit makes the archived-set-equals-deleted-set property
unachievable → requirements architect.
