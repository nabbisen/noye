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

3. **`current_head_hash` derives the head from the same walk.** Fetch
   the rows and call `walk_chain`; the head is the `row_hash` of the last
   row the walk reached, or `GENESIS_HASH` if it never advanced past
   genesis.

   > **Rewritten 2026-07-31.** This step previously specified a SQL
   > query — *the row whose `row_hash` is no other row's `prev_hash`.*
   > **That query is wrong**, and the dev team's escalation
   > (`.git-exclude/reviewed/027-subject-05-ruling-and-defect.md`) is the
   > record. It returns two rows after an ordinary mid-chain deletion —
   > T-22's own scenario — because the deleted row's predecessor and the
   > true tail have the same shape. Built as written, every audit write
   > after any deletion would have been refused.

   **One code path for chain order.** Two implementations of "what order
   is this chain in" is how G-30 happened; do not add a third.

   Why the *walk's* last row rather than the table's true latest: after a
   deletion, the true latest row is itself unreachable from genesis, so
   chaining onto it would orphan **every row written from then on**.
   Chaining onto the last reachable row keeps the live chain going and
   leaves the orphaned island permanently visible — damage stays visible
   and bounded, which is the property the product is sold on.

4. **A fork at write time does not refuse the write.** If the walk finds
   two rows sharing a `prev_hash`, continue on the branch `walk_chain`
   already picks (its `(action_time, id)` tiebreak), and log at error
   level. Verification reports the losing branch as orphaned.

   **Do not refuse.** A fork means either concurrent writers (excluded by
   DEC-004) or someone inserting rows directly — and that second case is
   the attacker the chain exists to detect. If a fork blocked audit
   writes, and audit writes gate mutations, anyone able to insert one row
   could freeze every mutation in the product. **An integrity control
   must not be convertible into a kill switch.** Nothing is lost by
   continuing: both branches stay in the table and the anomaly is
   reported on every verification.

5. **Every function that reads `audit_logs` must be total.** This is the
   standing rule both of this subject's escalations turned out to be
   instances of, and it outlives the subject:

   > `audit_logs` may contain rows `log()` did not write. That is not an
   > edge case — it is the module's entire reason to exist. Every
   > function reading the table must terminate and produce a **report**
   > for arbitrary row content: never a hang, never a refusal, never a
   > panic.

   **`walk_chain` currently violates this** — see T-23c. It is a
   regression: the code it replaced iterated a `Vec` with a bounded
   `for` and could not loop.

   > **Correction, 2026-07-31 (round 3).** `027` told you to report a
   > cycle as *"a `TamperedRow` with a distinct reason."* **That
   > instruction was wrong** — it double-classifies the revisited row and
   > breaks `external-design.md` S-11's "exactly one of four classes."
   > Measured on your own cycle fixture: `total=2` but `classified=3`,
   > with `r2` appearing twice in `tampered_rows`. My defect, not yours.
   >
   > A cycle is a property of the **chain's structure**, not of a row:
   >
   > - `ChainVerification` gains `cycle_at: Option<String>` — the id of
   >   the row where the loop closes, `None` normally.
   > - **Remove the second `TamperedRow` push.** Break and set
   >   `cycle_at`; the row keeps whatever class its first visit gave it.
   > - `/me/security` reports a non-`None` `cycle_at` on its own line.
   >   **An all-clear must be impossible while `cycle_at` is set**, the
   >   same rule already applied to `orphaned_rows`.
   >
   > `external-design.md` S-11 is amended already — read it before
   > building.

6. **Rewrite the concurrency note in `audit.rs`'s module docs.** It
   covers only the concurrent-writer race today. It must record that
   order comes from the links and never from a sort, **and why** —
   otherwise the next person to touch either query reintroduces this by
   "optimising" the tail query into an `ORDER BY … LIMIT 1`.

7. **`docs/src/security-posture.md`:** rows chained before this fix under
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
| T-23b | `current_head_hash` returns the true tail with 20 rows in one second; **and, after a mid-chain deletion, returns the last genesis-reachable row — not the orphaned island's tail** | guard |
| T-23c | `walk_chain` **terminates** on a genesis-rooted cycle (`r1: GENESIS→h1`, `r2: h1→h1`) and reports it distinctly from an ordinary tampered row | **must fail first — hangs today** |
| T-23e | **The partition holds on every fixture**: `verified + legacy + tampered.len() + orphaned.len() == total_rows`, and no id appears in both lists. Asserted by a shared helper called from **every** `walk_chain` test, not by one test of its own | **must fail first — the cycle fixture reports one row twice** |
| T-23d | A fork at write time does not refuse the write: `log` succeeds, chains onto the deterministically chosen branch, and the losing branch verifies as orphaned | guard |

**T-20 must loop.** The defect is UUID-ordering dependent; against the
broken code a single run of the two-row case passes about half the time.
Assert on the aggregate of at least ten runs. Twenty rows in one second
should fail **every** run against the pre-fix code — if your baseline
shows it passing even once, your harness is not reproducing the defect
and that is the finding.

**T-23e is the guard that would have caught this whole class.** Eight
tests asserted eight specific behaviours and none asserted the invariant
spanning them, which is why a duplicate survived a round of review by two
parties. Make it a helper every `walk_chain` test calls, not a ninth test
— an invariant checked in one place is a behaviour, not an invariant.

**T-23c is a defect in code already written**, independent of everything
else in this reissue, and can be worked immediately. Run it before the
fix and confirm it hangs — `timeout 30 cargo test …` exiting **124** is
the baseline, and a test that hangs the suite is captured with a timeout,
never left to run. Two rows are enough; no D1 is involved.

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
- **Any function reading `audit_logs` that you cannot convince yourself
  terminates for arbitrary row content** → stop and report. Build step 5
  is the rule; T-23c is what happens when it is missed. This escalation
  replaces the previous "the tail query returns more than one row" row,
  which was raised, ruled on, and is now Build steps 3 and 4.
- **If following the links turns out to be measurably slow** at
  realistic row counts, report the measurement. DEC-020 names an index on
  `prev_hash` as the first mitigation; do not reach for an ordering
  column.
