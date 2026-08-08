# 07c — Binding an `i64` to D1 is refused as a BigInt

**Milestone** M1.1 · **Closes** G-38 · **Blocks** 07a #2/#3/#4
**Branch** `fix/07c-i64-bind` · **Depends on** 07b
**Governing artifact** — Gap **G-38** (§11)

## The defect

`wasm-bindgen` converts Rust integers to `JsValue` two different ways
(`wasm-bindgen-0.2.122/src/lib.rs`):

```rust
integers!     { i8 u8 i16 u16 i32 u32 }   →  JsValue::from_f64(…)   // JS Number
big_integers! { i64 u64 }                 →  wbg_cast(…)            // JS BigInt
```

**D1's bind validation refuses a BigInt:**

```
D1_TYPE_ERROR: Type 'bigint' not supported for value '100'
    at D1PreparedStatement.bind (cloudflare-internal:d1-api:282:42)
```

So every `JsValue::from(<an i64>)` in a bind list fails at runtime. Every
`… as i32` cast is fine — the defect is precisely and only `i64`/`u64`.

### 23 binds, 10 statements, 6 modules

> **Corrected 2026-08-03.** This table read "nine sites" and was wrong in
> three places, all found by the dev team's own sweep and confirmed by
> re-reading each statement: `targets.rs` create was missing `port`;
> `targets.rs` update named three fields when it binds six; and
> **`results.rs::insert` was missing entirely** — the highest-frequency
> write in the system. A sweep is a snapshot and this one was mine; T-201
> exists so the final count is yours.

| Statement | Binds | Fields |
|---|---|---|
| `db/targets.rs` create | **6** | `port` :88, `expected_status` :90, `tls_threshold_days` :92, `timeout_sec` :93, `retry_count` :94, `interval_minutes` :95 |
| `db/targets.rs` update | **6** | `port` :131, `expected_status` :142, `tls_threshold_days` :148, `timeout_sec` :155, `retry_count` :156, `interval_minutes` :157 |
| **`db/results.rs` insert** | **3** | `status_code` :19, `response_time_ms` :23, `tls_days_left` :37 |
| `db/states.rs` update | 2 | `new_consecutive_successes` :129, `new_consecutive_failures` :130 |
| `db/incidents.rs` resolve | 1 | `duration` :36 |
| `db/incidents.rs` list | 1 | `limit` :61 |
| `db/results.rs` list_recent | 1 | `limit` :55 |
| `db/audit.rs` list_recent | 1 | `limit` :505 |
| `db/audit.rs` list_for_actor | 1 | `limit` :528 |
| `db/retention.rs` select | 1 | `RETENTION_BATCH_SIZE` :197 |

Everything binding `Option<String>` is safe — `body_contains`, `tags`,
`path`, `error_message`, `tls_expiry_date`, `details`, `note`,
`target_tag`, `previous_value`, `new_value`, `ip_address`. Every
`… as i32` is safe from `D1_TYPE_ERROR` (see **G-39** for why they are
not therefore correct).

**This is more severe than G-36.** G-36 broke reads of six tables; this
breaks the core write path *and* every paginated read. Not a regression —
it predates every release.

### `results.rs::insert` is the one to fix first

Once per check, per target, per interval. Any HTTP check that records a
status code fails today. It was absent from the original table, which is
why it gets its own test rather than riding along with the others.

### Target creation is broken on every path

`db/targets.rs:93` binds `input.timeout_sec.unwrap_or(10)` — an
unconditional `i64` — so creation fails regardless of what the caller
sends.

> **Corrected 2026-08-03.** This section previously said creation succeeds
> *without* an `expected_status` and fails *with* one. **It fails either
> way**, confirmed by the dev team testing both. The asymmetry at `:90` is
> real as a property of that one expression — its `Some` arm is a BigInt,
> its `200` fallback an `i32` — but I generalised from one expression to a
> nineteen-parameter statement without checking the other eighteen.

## ⛔ Step 0

Reproduce **one write-path site** — `targets.rs` create is the most
consequential — against `wrangler dev --local`, and capture the
`D1_TYPE_ERROR`. 07b already has the read-path reproductions; what has
not been shown is that a write fails the same way.

**If a write-path bind succeeds, stop and report.** That would mean D1
accepts BigInt on some paths and not others, and the fix below would be
wrong.

## Build

### One conversion helper, applied at every site

Do **not** scatter `as i32` casts. `i32` silently truncates above ~2.1
billion, and while nothing here plausibly exceeds it, a truncating cast
is the wrong shape for a boundary fix — it makes the failure silent
instead of loud, which is the trade this project keeps refusing.

Write one helper in `noye-shared` beside `bool_from_d1`, taking `i64` and
producing a `JsValue` D1 accepts, and **reject rather than truncate**
anything outside the safe-integer range. `f64` represents integers
exactly to 2^53; beyond that a JS Number is not the value you passed, and
silently storing a different number in a monitoring system is worse than
refusing.

`Option<i64>` sites need the same treatment for the `Some` arm, with
`JsValue::NULL` for `None` — and note that at `targets.rs:90,92` the
current `unwrap_or` fallbacks are literals that already work, so **do not
change the fallback values while fixing the arm beside them.**

### Do not

- **Do not change the struct field types to `i32`.** They cross the
  Gateway↔Core boundary and appear in the JSON API; `i64` is the right
  type for the domain, and this is a boundary-encoding defect, not a
  modelling one.
- **Do not patch or vendor `wasm-bindgen`.** Its `From` impls are
  correct for wasm; D1's bind validation is the constraint.
- **Do not fix only the sites listed.** The table is from a sweep of
  `JsValue::from(…)` in bind lists as the code stands today. **Re-run
  the sweep** as part of this subject and report any site the table
  misses — a table is a snapshot, and this one is mine.

## Verify

| # | Test | Type |
|---|---|---|
| T-194 | The helper converts `0`, `1`, and a large in-range `i64` to values D1 accepts, and **rejects** anything beyond 2^53 rather than truncating | **guard — critical** |
| T-195 | Creating a target **with** `expected_status` and `tls_threshold_days` set succeeds against local D1 — the asymmetry above | **must fail first** |
| T-196 | Updating a target's `timeout_sec`, `retry_count` and `interval_minutes` succeeds | **must fail first** |
| T-197 | A monitor state update writes `consecutive_successes`/`failures` | **must fail first** |
| T-198 | Resolving an incident writes `duration_sec` | **must fail first** |
| T-199 | Each paginated read returns rows: results, incidents, audit ×2 | **must fail first** |
| T-200 | `run_cleanup` completes a full pass against local D1 — **07b's T-192, finally reachable** | **must fail first** |
| T-201 | A re-run of the `JsValue::from(…)`-in-bind-list sweep finds no `i64`/`u64` site left unconverted | guard |
| T-202 | `results.rs::insert` writes a check result carrying `status_code`, `response_time_ms` **and** `tls_days_left` — the highest-frequency write, and absent from the original sweep | **must fail first** |

**T-194 is the one that matters**, and its second half more than its
first. A helper that truncates would pass every other test here while
storing a different number than the operator entered.

**T-200 unblocks 07a's #2, #3 and #4.** They have been blocked on G-36,
then on G-38; it is the same three residuals and this is the second cause.

## Done

- All eight tests pass; the five must-fail-first baselines captured
- `docs/src/requirements.md`: G-38 struck, mechanism recorded
- `CHANGELOG.md` — it publishes verbatim
- **07a's #2/#3/#4 handed back as unblocked**

## Escalate

- **A write-path bind that succeeds** in Step 0 → architect, before
  building.
- **Any site the sweep finds that the table above misses** → report it
  before fixing; the count matters to G-38's record.
- **Any value in these columns that could legitimately exceed 2^53** →
  architect. It would mean the helper's reject-don't-truncate rule needs
  a different answer than an error.
