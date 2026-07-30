# 07 — Audit write failures are surfaced

**Milestone** M1 · **Closes** G-26 · **Satisfies** FR-AUD-08, FR-AUD-11
**Branch** `fix/07-audit-surface` · **Depends on** 06 · **Per DEC-011**
**Governing artifact** — Gap **G-26** (§11) · **DEC-011** decides the failure policy

## The defect

All 19 audit call sites across
`crates/core/src/api/{targets,channels,incidents,maintenance,migration,users}.rs`
read:

```rust
let _ = db::audit::log(…).await;
```

The result is discarded without even a log line. Any transient failure
produces a mutation that happened and an audit row that does not exist —
and `verify_chain` still reports the trail intact, because the chain
covers only rows that were written.

The hash chain proves retained rows were not altered. It cannot prove
rows are not missing.

## Build

1. Add `audit::log_or_report(...)`: executes the write; on failure logs
   at error level with resource type, identifier, action and actor —
   **never the changed values** — and returns an indicator the caller
   propagates.
2. Replace all 19 sites. Call-site length must stay comparable: there
   must be no readability incentive to skip one.
3. Propagate the indicator to the Gateway and render it in the existing
   inline result region.

### Observable behaviour (external design §5.1, §4.5)

A mutation that succeeds but whose audit record fails returns **200 with
a warning indicator** — not an error. The mutation *happened*; a 500
would tell the operator the opposite, and there is no transaction to roll
back.

Required copy — change first, failure second, because the operator's
first question is "did it happen?":

> *"Change applied. It could not be written to the audit log — please
> record it manually."*

Calm and factual. No alarm decoration.

### Do not

- Do not make the mutation and the audit write atomic. That is RFC 0007,
  deliberately deferred — a cross-cutting refactor across eight `db::*`
  modules.
- Do not touch `api/audit.rs:81` (`log_login`); it already propagates.
- Error-level logging alone does **not** satisfy FR-AUD-11. Platform logs
  are not an operator-facing surface.

### Note

The point of the helper is that the compiler stops treating "ignore the
result" as the path of least resistance. **If a call site ends up wanting
`let _ =` again, the helper's signature is wrong — report it.**

## Verify

| # | Test | Type |
|---|---|---|
| T-30 | Forced audit failure → the target delete still deletes the target | guard |
| T-31 | …and the response carries a warning indicator | **must fail first** |
| T-32 | …and the UI renders it **alongside**, not instead of, the success message | **must fail first** |
| T-33 | …and an error-level log names resource type, id, action, actor | **must fail first** |
| T-34 | …and that log contains none of the changed values | **must fail first** |
| T-35 | No `let _ = db::audit::log` remains in the tree | **must fail first** |
| T-36 | The success path is behaviourally identical to today | guard |

**T-32 is a UI assertion.** FR-AUD-11 requires the failure be observable
to the *operator*. Assert against rendered HTML — the UI layer is pure
functions returning `String`, so no browser is needed.

## Done

- All seven tests pass; five baseline failures captured
- `docs/src/requirements.md`: FR-AUD-08, FR-AUD-11 → `Implemented`, G-26 struck

**→ Cut v0.28.2 (M1) after subjects 04–07 are merged.**

## Escalate

The helper's signature permitting `let _ =` → requirements architect.
