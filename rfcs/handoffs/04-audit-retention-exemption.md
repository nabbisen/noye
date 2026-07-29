# 04 — Audit rows are never deleted by retention

**Milestone** M1 · **Closes** G-04 · **Satisfies** FR-AUD-06, DR-LIF-04
**Branch** `fix/04-audit-retention-exempt` · **Depends on** 02
**Governing artifact** — Gap **G-04** (§11)

## The defect

`sql/0001_initial.sql:169` seeds `('audit_logs', 365, 1)` into
`retention_policies`, and `retention.rs` has an `audit_logs` arm that
deletes.

This directly contradicts the tamper-evidence design. After 365 days the
deletion breaks the hash chain, and the integrity check reports the
result as damaged — the product destroying its own evidence on a
schedule.

## Build

1. Migration `sql/0003_audit_retention_exemption.sql`:
   `DELETE FROM retention_policies WHERE table_name = 'audit_logs';`
   Idempotent by construction. **First migration after DEC-010 retired
   `0002` — the numbering gap is intentional.**
2. In the retention module, remove the `audit_logs` deletion arm **and**
   add a non-expiring data-class guard the pass refuses to delete from
   regardless of any policy row present. `audit_logs` is its only member.

### Why both

The row alone leaves code that a hand-inserted policy row would trigger.
The guard alone leaves a row asserting an intent the code contradicts.
Same pattern as subject 03: a safety property must not be conditioned on
configuration that can change.

### Do not

Do not add audit archival to R2. FR-AUD-09's off-system mirror is
RFC 0002's subject and is properly solved by log shipping.

## Verify

| # | Test | Type |
|---|---|---|
| T-16 | With an `audit_logs` policy row **manually reinserted**, a full pass deletes zero audit rows | **must fail first** |
| T-17 | `retention_policies` has no `audit_logs` row after migration | guard |
| T-18 | `check_results` and resolved `incidents` are still pruned | guard |
| T-19 | Chain verification is identical before and after a retention pass | guard |

**T-16 is the one that matters.** It tests the guard, not the absence of
configuration — which is what FR-AUD-06's amended acceptance criterion
now requires.

## Done

- All four tests pass; T-16's baseline failure captured
- `docs/src/requirements.md`: FR-AUD-06, DR-LIF-04 → `Implemented`, G-04 struck

## Escalate

Nothing anticipated. This is the smallest subject in M1.
