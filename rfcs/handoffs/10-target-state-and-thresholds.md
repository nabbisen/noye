# 10 — Target state row and threshold location

**Milestone** M2 · **Closes** G-06 · **Satisfies** DR-ENT-01, DR-ENT-04, FR-MIG-08
**Implements** [RFC 0008](../proposed/008-target-thresholds-on-target.md) (accepted, DEC-012)
**Branch** same as subjects 08–09 · **Depends on** 08, 09
**Governing artifact** — **RFC 0008** (accepted, DEC-012) · gap **G-06** (§11)

## The defects

Import creates no `target_states` row, so an imported target has no state
to look up and is **not monitorable** — the failure surfaces later, at
monitoring time, far from the import that caused it.

And `success_threshold` / `failure_threshold` live on `target_states`,
not `targets`, so they are absent from the configuration document and
silently reset to 3 on a round trip. A target deliberately configured to
fail over after one check comes back failing over after three.

## Build

**Migration `sql/0005`** — RFC 0008:

1. `ALTER TABLE targets ADD COLUMN success_threshold INTEGER NOT NULL DEFAULT 3;`
2. Same for `failure_threshold`.
3. Copy existing values across by `target_id`.
4. Rebuild `target_states` without the two columns.

Step 4 is not optional. A duplicated configuration value with no defined
authority is exactly the ambiguity this removes.

**Shared type.** `Target` gains both fields; `TargetState` loses them.
Export and import then carry them with no further work.

> **⚠️ Both new fields are integers, and every bind of them is a new
> G-38 site.** This subject predates G-38: binding an `i64` raw produces a
> JS `BigInt`, which D1 **refuses outright**. Route
> `success_threshold` and `failure_threshold` through
> `noye_shared::i64_to_d1`/`opt_i64_to_d1` wherever they are bound —
> `db/targets.rs` create and update, and `db/migration.rs`'s import path,
> which already uses the helpers at seven sites (subject 07d, G-39).
> Do not add an `as i32` cast instead; that is G-39, which truncates in
> silence. See `docs/src/d1-type-boundary.md`.

**Read path.** `decide_transition` already takes thresholds as arguments
and is pure (FR-MON-07). Only its **caller** changes, to read from the
target rather than the state row.

**Import.** Create the `target_states` row in the same operation as the
target — counters at zero, status `unknown` — exactly what
`db::targets::create` does on the normal path
(`crates/core/src/db/targets.rs:98`).

### ⛔ Stop and report

**`decide_transition`'s existing unit tests must pass unmodified.** If
you find yourself editing one, stop — the transition logic is not in
scope, and a change there means something unauthorised has been altered.

### Note

Thresholds are *configuration*, not state. Every other decision criterion
— expected status, body substring, TLS threshold, timeout, retries,
interval — is already on `targets`. With this change `target_states`
becomes fully derived: delete it and monitoring rebuilds it from the
next check.

## Verify

| # | Test | Type |
|---|---|---|
| T-46 | Import into an empty database, then run one monitor tick — the imported target is selected and probed | **must fail first** |
| T-47 | Export → import → export reproduces a non-default `failure_threshold` exactly | **must fail first** |
| T-48 | An imported target with `failure_threshold = 1` goes `down` after one failed check, not three | **must fail first** |
| T-49 | Existing `decide_transition` unit tests pass **unmodified** | **guard — critical** |
| T-50 | After migration `0005`, no threshold column remains on `target_states` | guard |
| T-51 | Thresholds configured before migration `0005` survive it | guard |
| T-51a | 07c's sweep — `JsValue::from(…)` in a bind list — finds **no new `i64`/`u64` site** after this subject; both new threshold fields bind through `i64_to_d1` | **guard — critical** |

**T-46 is FR-MIG-08's acceptance criterion stated as a test.** Not "a row
exists" — run a monitor tick and assert the target was probed. That
requirement says an imported object must be equivalent to one created
normally, and "is it monitored?" is the only end-to-end check of it.

**T-49 is a scope guard.** RFC 0008 moves where thresholds are *stored*;
it changes nothing about how they are *applied*.

## Done

- All six tests pass; three baseline failures captured
- RFC 0008 → `rfcs/done/`, `Status: Implemented (0.29.0)`, inbound links fixed
- `docs/src/requirements.md`: DR-ENT-01, DR-ENT-04, FR-MIG-08 →
  `Implemented`, G-06 struck
- `docs/src/migration.md` updated
- `CHANGELOG.md`: configuration document format changed

## Escalate

`decide_transition` needing modification → requirements architect.
