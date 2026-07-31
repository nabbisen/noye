# RFC 0007: Atomic audit writes

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-07-28
**Related ROADMAP item**: "Atomic audit writes" under `## Operations infrastructure`
**Estimated size**: medium (~3 days on top of the surface-and-complete baseline)
**Implementation target**: post-0.29.0

---

## Summary

Make a business mutation and its audit record land atomically, so that
Noye can state a guarantee it currently cannot: **no change occurs
without a record of it.** Today the two writes are independent, and
DEC-011 accepts that an audit failure leaves the mutation applied but
unrecorded, with a warning to the operator.

## Background

FR-AUD-08 requires that a failed audit write be surfaced rather than
silently discarded. Before v0.28.2 it was neither surfaced nor recorded:
all nineteen call sites read `let _ = db::audit::log(…).await`, so a
failure was discarded without even a log line (gap G-26).

v0.28.2 closes FR-AUD-08 by the *surface and complete* route (DEC-011):
the mutation completes, the failure is logged at error level, and the
operator sees a warning in the operation's result panel.

That satisfies the requirement as written. It does not satisfy a
stronger property that has never been stated as a requirement:

> A state-changing operation MUST NOT take effect unless its audit
> record is written.

This RFC exists so that property is *tracked rather than assumed*. It
was considered and deliberately deferred during the Phase 1 audit
remediation, on the grounds that a three-day cross-cutting refactor does
not belong inside a repair phase.

### Why "fail the operation" is not a smaller alternative

An earlier draft of the remediation guidance proposed simply returning
an error when the audit write fails. That does not work. The mutation
executes first and returns; the audit insert runs afterwards. Failing at
that point reports failure for something that already happened —
strictly worse than reporting nothing, because the operator will
reasonably retry.

Atomicity is not an optimisation of that approach. It is the only shape
in which failing the operation is honest.

## Design

### Mechanism

D1 executes `db.batch(...)` as a single transaction. Placing the
business statement and the audit statement in one batch makes them
atomic without introducing a new dependency or a Durable Object.

### Required refactor

The blocker is structural, not conceptual. Today each `db::*` mutation
prepares *and executes* its own statement:

```rust
pub async fn delete_target(db: &D1Database, id: &str) -> Result<()> {
    db.prepare("DELETE FROM targets WHERE id = ?1")
        .bind(&[id.into()])?
        .run()
        .await?;
    Ok(())
}
```

For a batch, the statement must be *returned* rather than run, so the
caller can compose it with the audit statement:

```rust
pub fn delete_target_stmt(db: &D1Database, id: &str) -> Result<D1PreparedStatement>
```

Approximately eight `db::*` modules are affected: `targets`,
`channels`, `incidents`, `maintenance`, `users`, `migration`, `states`,
`results`.

### Chain-head ordering

`audit::log` reads the chain head immediately before insertion. In a
batched design the head must be read *before* the batch opens, which
widens the window between reading the head and committing the row.

This is acceptable **only while assumption A-05 (a single writer) holds**.
The existing single-writer constraint from DEC-004 already governs this;
this RFC does not relax it, and must not be read as a step toward Queue
fan-out. Fan-out still requires a Durable Object or another
serialization point, and would need to land first.

Note also gap G-30: the head-selection query and the verification query
must use the same total order including tie-breaking. That is fixed in
v0.28.2 and must be preserved here.

### Behaviour change

| | Before (DEC-011) | After |
|---|---|---|
| Audit write fails | Mutation applied; 200 with a warning | Mutation **not** applied; error returned |
| Operator-visible contract | "Done, but not recorded" | "Nothing happened; retry" |
| External design §5.1 | Unrecorded-mutation warning documented | Warning row removed; a new failure mode documented |

This is an **externally visible contract change** and requires the
external design to be amended before implementation, per its §14.

## Requirements

If adopted, add:

> **FR-AUD-12** — A state-changing operation MUST NOT take effect unless
> its audit record is written in the same transaction.
>
> *Acceptance:* with the audit insert forced to fail, the corresponding
> business mutation is absent from the database.

And withdraw FR-AUD-11 (operator-visible audit-failure warning), which
this supersedes — marked `Withdrawn`, not deleted, per §15.

## Test plan

- With the audit insert forced to fail, a target delete leaves the
  target present.
- …and a channel update leaves the prior values intact.
- …and the response is an error, not a 200 with a warning.
- Chain verification is clean after a batch of mutations, repeatably
  across at least 10 runs (guards against reintroducing G-30).
- The success path is behaviourally identical to before.
- Existing FR-AUD-01 coverage — one audit row per operation — still
  passes for all eight affected modules.

## Security considerations

Strengthens the audit guarantee from *evidentiary best-effort* to
*transactional*, which is the property an external auditor would assume
the hash chain already implies. The chain proves retained rows were not
altered; it has never proved rows are not missing. This closes the
second half.

One caution: an audit-write failure now blocks the operator. During an
incident, an admin unable to resolve an incident because D1 is
struggling is a real operational cost, and it is the reason DEC-011 was
taken first. That trade should be re-argued, not assumed, when this RFC
is scheduled.

**This RFC reverses DEC-011 for a failed write. It does not — and must
not be read to — reverse DEC-020's fork ruling.** A *failed* audit write
is an infrastructure condition, and refusing the mutation is a defensible
response to it. A *forked* chain is an attacker-influenced condition:
under DEC-020, `log` continues on the chosen branch and reports, because
refusing would let anyone able to insert one row into `audit_logs` freeze
every mutation in the product. Adopting atomicity must leave that
behaviour intact — an integrity control must not become a kill switch,
whichever way the DEC-011 trade is settled.

## Out of scope

- Workers Queue fan-out and the Durable Object the chain would need for
  it. Unrelated, and a prerequisite rather than a consequence.
- Off-system audit mirroring — RFC 0002.
- Failed-login audit recording — RFC 0004.
- Any change to the canonical serialization or the hash format. The
  serialization is version-tagged and pinned by unit tests; this RFC
  changes *when* a row is written, never *what* it contains.
