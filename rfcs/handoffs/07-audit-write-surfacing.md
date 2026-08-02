# 07 — Audit write failures are surfaced

**Milestone** M1 · **Closes** G-26 · **Satisfies** FR-AUD-08, FR-AUD-11
**Branch** `fix/07-audit-surface` · **Depends on** 06 · **Per DEC-011**
**Governing artifact** — Gap **G-26** (§11) · **DEC-011** decides the failure policy

## The defect

**Seventeen** call sites discard the result of an audit write:

| Where | Count | Function |
|---|---|---|
| `crates/core/src/api/channels.rs` | 7 | `log` |
| `crates/core/src/api/targets.rs` | 3 | `log` |
| `crates/core/src/api/migration.rs` | 2 | `log` |
| `crates/core/src/api/users.rs` | 1 | `log` |
| `crates/core/src/api/incidents.rs` | 1 | `log` |
| `crates/core/src/api/maintenance.rs` | 1 | `log` |
| **`crates/core/src/monitor/engine.rs`** | **2** | **`log_system`** |

> **Corrected 2026-08-02 by pre-flight.** This section said *"all 19 call
> sites across `crates/core/src/api/{…}`"*. The count is **17**, and two of
> them are **not in `api/` at all** — they are the `log_system` calls in
> `monitor/engine.rs` that subject 06 (T-29) just verified are still
> wired. The old scope would have left them behind **and then failed
> T-35**, whose grep matches `log_system` too. Build and Verify
> contradicted each other.

All of them read:

```rust
let _ = db::audit::log(…).await;      // or log_system(…)
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
2. Replace all **17** sites. Call-site length must stay comparable: there
   must be no readability incentive to skip one.
3. Propagate the indicator to the Gateway and render it in the existing
   inline result region — **for the fifteen `api/` sites.**

### The two `log_system` sites are different, and must not be forced into the same shape

`monitor/engine.rs`'s two calls run from the **cron-driven monitor**.
There is no HTTP request, no response, and no inline result region to
render a warning into. Steps 1 and 3 cannot apply to them unchanged.

| | `api/` — 15 sites | `monitor/engine.rs` — 2 sites |
|---|---|---|
| Error-level log naming resource, id, action, actor (**FR-AUD-08**) | yes | **yes** |
| Warning indicator in the response (**FR-AUD-11**) | yes | **not applicable — there is no response** |

**FR-AUD-11 is about an operator seeing the failure of something they
just did.** Nobody is watching a cron invocation, so there is no surface
to render onto and inventing one is out of scope here.

**Do not skip them, and do not invent a surface for them.** Give them the
helper and the error-level log; leave the operator-facing half to the
fifteen. If that leaves you wanting a second helper with a different
signature, that is a legitimate outcome — say so rather than bending one
signature to cover both.

**These two only started mattering four days ago.** Before subject 06 they
could never succeed — the foreign key refused every one — so a discarded
result was discarding a guaranteed failure. Now they can succeed, which
means they can also fail *transiently*, and that failure currently goes
nowhere.

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
| T-35 | No `let _ =` on **any** `db::audit::log*` call remains in the tree — `log` and `log_system` both. Grep for the discard, not for one function name | **must fail first** |
| T-36 | The success path is behaviourally identical to today | guard |

**T-30 through T-34 and T-36 exercise the `api/` path.** For the two
`monitor/engine.rs` sites, assert only the FR-AUD-08 half — a forced
failure produces the error-level log with resource, id, action and actor,
and no changed values. There is no response and no UI to assert against,
and a test that invents one is testing the test.

**T-32 is a UI assertion.** FR-AUD-11 requires the failure be observable
to the *operator*. Assert against rendered HTML — the UI layer is pure
functions returning `String`, so no browser is needed.

## Done

- All seven tests pass; five baseline failures captured
- `docs/src/requirements.md`: FR-AUD-08, FR-AUD-11 → `Implemented`, G-26 struck

**→ Cut v0.29.0 (M1) after subjects 04–07 are merged.**

## Escalate

The helper's signature permitting `let _ =` → requirements architect.
