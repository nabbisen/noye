# The D1 type boundary

D1 is a real JavaScript runtime with its own type rules — not a
transparent SQLite wrapper, and not accurately described by SQLite's
own documentation alone. Three severe defects in this project (G-36,
G-38, and the finding below that became DEC-023) all had the same
shape: a Rust type crossing into JS as something D1 does not accept,
or coming back as something Rust cannot faithfully deserialize. Two of
them made the service completely non-functional and survived four
milestones and three releases before anyone ran the boundary rather
than assumed it.

This page exists so the next person adding a column reads one page
instead of rediscovering a gap entry. Every row below states its
evidence and date; a row that says "confirmed against the local D1
runtime" was run, not argued. Produced by subject 07d
(`rfcs/handoffs/07d-d1-type-boundary-audit.md`).

## The central fact

**Integers cross this boundary exactly only within ±2^53, in both
directions** (DEC-023). D1 surfaces every numeric column as a JS
Number — an `f64` — so an `INTEGER` value beyond `±2^53` is already
imprecise the moment D1 hands it back, before any Rust code runs:
inserting `i64::MAX` (`9223372036854775807`) via raw SQL and reading
it back produces `9.223372036854776e+18`. The precision is destroyed
at the platform boundary, not by this project's deserializer.

This makes it categorically different from G-36 and G-38, both of
which were *encoding* defects with a Rust-side fix that recovers the
true value (a `Visitor`, a converter). **There is no Rust-side fix
here.** No amount of deserializer cleverness reconstructs an integer
that already arrived as a rounded float.

**Writes enforce the limit already**: `noye_shared::i64_to_d1` /
`opt_i64_to_d1` (subject 07c) reject, rather than truncate, anything
outside `±2^53` before it ever reaches a bind. **Reads cannot enforce
it — they can only report a violation that already happened.** Reading
an out-of-range `INTEGER` into a typed `i64` field traps at `worker`'s
internal `.unwrap()` (the same unloggable shape as G-36) rather than
returning a catchable error — registered as G-41, Low, because no code
path in this project can currently produce such a value: `i64_to_d1`
refuses to write one, every aggregate this project reads into `i64` is
a `COUNT`/`SUM` bounded by row count, and no domain column's plausible
range approaches the limit. The only route in is an operator running
`wrangler d1 execute` or a hand-written migration directly.

**If a future requirement genuinely needs an integer beyond `±2^53`**,
the answer is `TEXT` storage with explicit parsing, not a cleverer
deserializer — the loss happens before Rust is involved, so there is
nothing for a deserializer to recover.

## Bind direction — Rust → D1

| Rust type | What D1 receives | Accepted? | What this codebase uses | Evidence |
|---|---|---|---|---|
| `&str` / `String` | JS String | Yes | `.into()` everywhere | In continuous production use |
| `i64` (raw, via `JsValue::from`) | JS `BigInt` (`wasm-bindgen`'s `big_integers!` macro) | **No** — `D1_TYPE_ERROR: Type 'bigint' not supported` | Never bound raw; see `i64_to_d1` below | Confirmed against the local D1 runtime, 2026-08-08 (G-38); reconfirmed as this document's control, 2026-08-11 |
| `u64` (raw, via `JsValue::from`) | JS `BigInt`, identically to `i64` | **No** — identical `D1_TYPE_ERROR`, identical bind-site stack | Not used in this codebase today; would need `i64_to_d1`-equivalent treatment if introduced | Confirmed against the local D1 runtime, 2026-08-11 |
| `i64` via `noye_shared::i64_to_d1` / `opt_i64_to_d1` | JS Number, via `JsValue::from_f64` | Yes, within `±2^53`; the helper itself refuses anything wider rather than binding it | Every `i64`/`Option<i64>` bind in `noye-core` (subject 07c, 23 sites) | Confirmed against the local D1 runtime, 2026-08-08 |
| `i32` and smaller (incl. `bool as i32`) | JS Number, via `JsValue::from_f64` | Yes | `is_disabled as i32`-style casts throughout `db/`; `db/migration.rs`'s import path (see G-39, closed by this subject's Step 3) | Confirmed against the local D1 runtime, 2026-08-11 (control) |
| `f64` | JS Number | Yes | Not bound anywhere in this codebase today (no `REAL` column exists) — this is a scratch-table finding, ready for the day one does | Confirmed against the local D1 runtime, 2026-08-11 |
| `Vec<u8>` (via `js_sys::Uint8Array::from(&bytes)`) | a JS `Uint8Array` | Yes | Not bound anywhere in this codebase today (no `BLOB` column exists) — this construction is untested by any other precedent in the codebase and should be treated as the reference pattern, not re-derived, if a `BLOB` column is introduced | Confirmed against the local D1 runtime, 2026-08-11 |
| `JsValue::NULL` | SQL `NULL` | Yes | Every `Option::None` bind, via `.map(JsValue::from).unwrap_or(JsValue::NULL)` or `opt_i64_to_d1(None)` | Confirmed against the local D1 runtime, 2026-08-11 (control) |

## Read direction — D1 → Rust

| SQLite storage class | Arrives as (JS) | Deserializes cleanly into | What this codebase uses | Evidence |
|---|---|---|---|---|
| `INTEGER` | JS Number (`f64`) | `i64`, `i32` — yes. `bool` — **no**, traps | `noye_shared::bool_from_d1` (a `serde::de::Visitor`; subject 07b) on every `bool`-typed field backed by an `INTEGER` column | Confirmed against the local D1 runtime, 2026-08-03 (G-36); reconfirmed as this document's control, 2026-08-11 |
| `INTEGER`, beyond `±2^53` | JS Number, **already imprecise** — `i64::MAX` arrives as `9.223372036854776e+18` | `i64` — traps (G-41); the float itself is already wrong regardless | Not applicable — no code path in this project can produce one (see "The central fact" above) | Confirmed against the local D1 runtime, 2026-08-11 |
| `TEXT` | JS String | `String`, `Option<String>` | Throughout `db/` | In continuous production use |
| `NULL` | JS `null` | `Option<T>` for any `T` | Throughout `db/` | Confirmed against the local D1 runtime, 2026-08-11 (control) |
| `REAL` | JS Number | `f64` | No `REAL` column exists in this schema today | Confirmed against the local D1 runtime, 2026-08-11 |
| `BLOB` | **A plain JS Array of byte values (0–255 each)** — *not* an `ArrayBuffer`, *not* a `Uint8Array` wrapper, *not* base64 | `Vec<u8>` | No `BLOB` column exists in this schema today | Confirmed against the local D1 runtime, 2026-08-11 |

## Controls (T-205)

Every "confirmed" row above rests on the harness having been proven to
reproduce known behavior before being trusted on unknown behavior.
Three controls were run first: `i64` raw bind reproduces G-38's exact
`D1_TYPE_ERROR` and bind-site stack; `INTEGER` deserialized into a raw
`bool` reproduces G-36's exact panic site
(`worker-0.8.5/src/d1/mod.rs:491:69`); `i32` bind succeeds cleanly,
confirming G-39's "safe from `D1_TYPE_ERROR`" claim. All three matched
exactly — see
`.git-exclude/evidence/subject-07d-step1-boundary-audit.log`.

## Related

- **[Development § Node is not Workers](./development.md)** — why every row here says *local D1 runtime* and not *Node*, and why that distinction is load-bearing
- **DEC-023** — the ±2^53 boundary, `docs/src/decision-log.md`
- **G-36** (closed), **G-38** (closed), **G-39** (closed by this
  subject), **G-41** (open, Low) — `docs/src/requirements.md` §11
- A regression test asserting the load-bearing answers on this page
  lives in `crates/shared/src/lib.rs` (`d1_boundary_regression_tests`)
  and runs in CI's `wasm-tests` job — if a future `worker` or
  `wrangler` version changes any of them, that test fails and this
  page needs updating, not the other way around.
