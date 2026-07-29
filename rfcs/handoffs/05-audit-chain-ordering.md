# 05 — Writer and verifier agree on chain order

**Milestone** M1 · **Closes** G-30 · **Satisfies** FR-AUD-03, FR-AUD-04
**Branch** `fix/05-chain-order` · **Depends on** nothing
**Work this before subject 06.**
**Governing artifact** — Gap **G-30** (§11)

## The defect

Two functions disagree on row order:

| Function | Ordering |
|---|---|
| `current_head_hash` — picks the row a new entry chains **to** | `ORDER BY action_time DESC LIMIT 1` |
| `verify_chain` — walks the chain to check it | `ORDER BY action_time ASC, id ASC` |

`action_time` is second-resolution. When two rows share a second — a
configuration import writes two — the writer's tiebreak is unspecified
while the verifier's is `id ASC` on a **random UUID**.

Whenever the second-written row draws the smaller UUID, it is verified
*before* the row it chained to, fails linkage, and pins every subsequent
row as tampered. **A routine same-second pair has roughly a 1-in-2 chance
of reporting the entire remaining trail as tampered.**

This is not the concurrent-writer race already noted in `audit.rs`. It
happens with a single writer, sequentially.

A tamper-evidence control that cries wolf is as damaging as one that
stays silent: the first false positive teaches the operator to disbelieve
it.

## Build

1. `current_head_hash` → `ORDER BY action_time DESC, id DESC LIMIT 1` —
   the exact reverse of the verifier, so the head is always the
   verifier's last row.
2. Rewrite the concurrency note in `audit.rs`'s module docs. It covers
   only the concurrent-writer race today. It must also record that
   ordering ties are resolved identically at both ends, **and why** —
   otherwise the next person to touch either query reintroduces this.
3. `docs/src/security-posture.md`: document that rows chained before this
   fix under a same-second tie may verify as tampered, and that the
   condition cannot be repaired without rewriting stored hashes, which
   would defeat the property being asserted.

### Why before subject 06

Subject 06 rewrites the hash-chained table and must prove chain
classification is unchanged across the migration. Doing that against a
verifier which is itself producing false positives proves nothing.

## Verify

| # | Test | Type |
|---|---|---|
| T-20 | 20 audit rows written within one second verify clean — **≥10 runs** | **must fail first** |
| T-21 | Head-selection and verification queries assert the same total order | guard |
| T-22 | Mid-chain row deletion is still reported as tampered | **guard — critical** |
| T-23 | A value alteration in a row is still reported as tampered | **guard — critical** |

**T-20 must loop.** The defect is UUID-ordering dependent; a single run
has roughly even odds of passing against the broken code. Assert on the
aggregate of at least ten runs.

**T-22 and T-23 are non-negotiable.** Every change in M1 makes the
verifier more permissive in some direction. These two prove it still
catches real tampering. If either goes green when it should be red, that
change has broken the property the product is sold on and does not ship.

## Done

- All four tests pass; T-20's baseline failure captured across ≥10 runs
- `docs/src/requirements.md`: FR-AUD-03 → `Implemented`, G-30 struck

## Escalate

T-22 or T-23 failing at any point, for any reason → requirements architect.
