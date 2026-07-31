# 05 — The chain carries its own order

**Milestone** M1 · **Closes** G-30 · **Satisfies** FR-AUD-03, FR-AUD-04, FR-AUD-05
**Branch** `fix/05-chain-order` · **Depends on** nothing
**Work this before subject 06.**
**Governing artifact** — Gap **G-30** (§11), **DEC-020**

> **Reissued 2026-07-31.** The first version of this subject specified a
> fix that does not work, and a test that its own fix guaranteed would
> fail. It was withheld before reaching you; nothing was built against
> it. The analysis is
> `.git-exclude/reviewed/025-subject-05-defective-fix.md` and it is worth
> ten minutes before you start — the reasoning behind this design is
> mostly the reasoning about why the obvious one fails.

## The defect

`verify_chain` reconstructs the chain's order by **sorting stored
columns**:

```sql
SELECT * FROM audit_logs ORDER BY action_time ASC, id ASC
```

The chain's order is **insertion order**. Recovering insertion order by
sorting requires a sort key that is monotonic with insertion, and neither
column is:

- `action_time` is second-resolution (`audit.rs:161`)
- `id` is `Uuid::new_v4()` — **random** (`audit.rs:159`)

So a row written into a second that already has rows in it lands at a
random position among them. Whenever it sorts before the row it chained
to, its `prev_hash` matches nothing the walk expects, and — because the
verifier deliberately pins `expected_prev` on failure — **every
subsequent row is reported as tampered too.**

Measured against the current code:

| Rows written in one second | Verify clean |
|---|---|
| 2 — *a configuration import writes two* | ~51% |
| 5 | ~0.4% |
| 20 | **0%** |

A tamper-evidence control that cries wolf is as damaging as one that
stays silent: the first false positive teaches the operator to disbelieve
it. This happens with a single writer, sequentially — it is not the
concurrent-writer race already noted in `audit.rs`.

### Why matching the tie-breaks is not the fix

The obvious repair is to make `current_head_hash` order by
`action_time DESC, id DESC`, the exact reverse of the verifier, so the
head is always the verifier's last row. **That was specified, and it does
not work.** It fixes *which row is chosen as head*. It does nothing about
the new row's own sort position, which is still random — so the new row
still lands before its own predecessor about half the time. For the
two-row import case it changes nothing whatsoever.

Any fix that keeps "sort the rows to recover the order" only narrows the
window. Sub-second timestamps narrow it further and still do not close
it, and `action_time` is a hashed field.

## Build

**Stop recovering the order. Read it from the links that already carry
it** (DEC-020).

1. **`verify_chain` follows the chain.** Load the rows (it already loads
   them all — no new I/O), index them by `prev_hash`, start at
   `GENESIS_HASH`, and follow `prev_hash → row_hash` link by link. The
   `ORDER BY` becomes irrelevant to correctness; keep a stable one only
   so output is deterministic.

2. **Four classes, not three** — FR-AUD-05 and `external-design.md` S-11
   are already amended:

   | Class | Condition |
   |---|---|
   | `verified` | Reached from genesis, and re-hashes to its stored `row_hash` |
   | `legacy` | Both hash columns null. Counted, never walked — as today |
   | `tampered` | **Reached**, but does not re-hash to its stored `row_hash` |
   | `orphaned` | Carries hashes, **not reached** from genesis |

   **`orphaned` must not be folded into `tampered`.** A deleted row makes
   its successors unreachable; calling them "tampered" names the wrong
   rows as damaged. This distinction is the requirement, not a nicety.

3. **`current_head_hash` stops sorting.** The tail is the row whose
   `row_hash` is no other row's `prev_hash`:

   ```sql
   SELECT row_hash FROM audit_logs
   WHERE row_hash IS NOT NULL
     AND row_hash NOT IN (SELECT prev_hash FROM audit_logs WHERE prev_hash IS NOT NULL)
   ```

   Genesis behaviour is unchanged: empty table, or only legacy rows,
   yields `GENESIS_HASH`.

   **If this query returns more than one row, the chain has forked.** Do
   not silently pick one. Decide and report what `log` should do —
   see Escalate.

4. **Rewrite the concurrency note in `audit.rs`'s module docs.** It
   covers only the concurrent-writer race today. It must record that
   order comes from the links and never from a sort, **and why** —
   otherwise the next person to touch either query reintroduces this by
   "optimising" the tail query into an `ORDER BY … LIMIT 1`.

5. **`docs/src/security-posture.md`:** rows chained before this fix under
   a same-second tie may already be recorded in an order the old verifier
   misread. Following the links **repairs the reading**, not the data —
   state that no stored hash is rewritten, and that rewriting stored
   hashes to make a chain verify would defeat the property being
   asserted.

### Do not

- **Do not add a `seq` column, or any other ordering column.** Considered
  and rejected in DEC-020: it needs a migration competing with subject
  06's `0004`, a backfill whose only correct order is the chain itself,
  and it cannot go into the canonical serialization without breaking
  DEC-005's pinned 11-field format — leaving an unhashed column that can
  be edited to disagree with the chain.
- **Do not change the canonical serialization or the hash format.** This
  subject changes *how rows are read back*, never what is stored.
- **Do not change `action_time`'s resolution.** It is a hashed field, and
  finer resolution is a mitigation, not a fix.
- **Do not "repair" a failing chain.** Reporting is the product.

## Verify

| # | Test | Type |
|---|---|---|
| T-20 | 20 audit rows written within one second verify clean — **≥10 runs** | **must fail first** |
| T-21 | Verification result is unchanged when rows are returned in a different `ORDER BY` — the walk does not depend on the query's order at all | guard |
| T-22 | Mid-chain row **deletion**: the deleted row's successors are `orphaned`, and **no row is `tampered`** | **guard — critical** |
| T-23 | A value alteration in a row is reported as `tampered`, and **only that row** | **guard — critical** |
| T-23a | Two rows sharing a `prev_hash` (a fork) leave one branch `orphaned`, count non-zero | guard |
| T-23b | `current_head_hash` returns the true tail with 20 rows in one second — the row no other row chains from | guard |

**T-20 must loop.** The defect is UUID-ordering dependent; against the
broken code a single run of the two-row case passes about half the time.
Assert on the aggregate of at least ten runs. Twenty rows in one second
should fail **every** run against the pre-fix code — if your baseline
shows it passing even once, your harness is not reproducing the defect
and that is the finding.

**T-21 is the one that proves the design.** If the result changes when
the `ORDER BY` changes, order is still being recovered from columns and
the fix is incomplete regardless of what the other tests say.

**T-22 and T-23 are non-negotiable, and both are now stricter.** They no
longer only require detection — they require the **right row** to be
named. Every change in M1 makes the verifier more permissive in some
direction; these prove it still catches real tampering and still points
at the correct row. If either goes green when it should be red, that
change has broken the property the product is sold on and does not ship.

## Done

- All six tests pass; T-20's baseline failure captured across ≥10 runs
- `docs/src/requirements.md`: FR-AUD-03 and FR-AUD-05 → `Implemented`,
  G-30 struck
- `CHANGELOG.md` — and note it is now published verbatim (subject 04a)

## Escalate

- **T-22 or T-23 failing at any point, for any reason** → requirements
  architect, immediately.
- **The tail query returns more than one row** in any test, or you
  believe it can under the single-writer constraint (DEC-004) → stop and
  report. A fork means the chain has two heads and `log` has no correct
  behaviour to fall back on; choosing one silently is the worst option
  and I would rather decide it than have it improvised.
- **If following the links turns out to be measurably slow** at
  realistic row counts, report the measurement. DEC-020 names an index on
  `prev_hash` as the first mitigation; do not reach for an ordering
  column.
