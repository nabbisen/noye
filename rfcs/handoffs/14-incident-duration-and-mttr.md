# 14 — Automatic resolution records a duration

**Milestone** M2 · **Closes** G-10 · **Satisfies** FR-INC-08
**Branch** `fix/14-incident-duration` · **Depends on** subject 13
**Governing artifact** — Gap **G-10** (§11)

## The defect

`crates/core/src/db/incidents.rs:26-27` sets `duration_sec` on manual
resolution. Line 44 — automatic resolution — does not. And
`crates/core/src/stats.rs` builds MTTR with
`filter_map(|i| i.duration_sec)`, so automatically-resolved incidents,
the overwhelming majority, contribute nothing.

The displayed MTTR is not merely incomplete. It is computed over an
unrepresentative minority and presented as if it were the whole picture —
**misleading rather than missing.**

## Build

1. Compute and store `duration_sec` on the automatic path exactly as the
   manual path does.
2. In `stats.rs`, derive duration from `resolved_at − opened_at` when the
   column is null, so rows written before this fix are not permanently
   excluded from reporting.

> **⚠️ "Exactly as the manual path does" includes how it binds.**
> `db/incidents.rs:36` routes the duration through
> `i64_to_d1(duration)`, not `JsValue::from`. An `i64` bound directly
> becomes a JS **BigInt**, which D1 refuses — that is **G-38**, and it
> fails as an unloggable trap rather than an error. See
> `docs/src/d1-type-boundary.md`. Computing it in SQL instead
> (`duration_sec = ...` inside the `UPDATE`) avoids the bind entirely and
> is equally acceptable; what is not acceptable is a raw `i64` bind.
>
> Note that `auto_resolve` updates **every** open incident for the target
> in one statement, so a SQL-side computation must reference each row's
> own `opened_at`, not a value computed once in Rust.

## Verify

| # | Test | Type |
|---|---|---|
| T-73 | An auto-resolved incident contributes to MTTR | **must fail first** |
| T-74 | A window containing only auto-resolved incidents returns an MTTR value, not none | **must fail first** |
| T-75 | A pre-existing row with a null `duration_sec` still contributes | **must fail first** |
| T-75a | A **real** auto-resolve through the running service produces a non-null `duration_sec` and a non-null MTTR | **must fail first** |

T-73–T-75 are host tests over fixtures in `stats.rs`. **T-75a is not** —
add it to `scripts/check-d1-behaviour.sh`, because the new write crosses
the D1 type boundary and a fixture cannot tell you whether D1 accepted
the value. Drive a target down and then up through a scheduled tick, as
assertion (c) already does, and read the incident back.

## Done

- All four tests pass; four baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` — the wasm suites, not just `cargo check`
- `docs/src/requirements.md`: FR-INC-08 → `Implemented`, G-10 struck
