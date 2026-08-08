# 07d — Characterise the D1 type boundary by exercising it

**Milestone** M1.1 · **Closes** G-39 · **Produces** `docs/src/d1-type-boundary.md`
**Branch** `fix/07d-d1-boundary` · **Depends on** 07b, 07c
**Governing artifact** — `.git-exclude/reviewed/042-subject-07c-closed.md` §Next

## Why this exists

Three defects in four days, all the same class — **a Rust type crossing
into JS as something D1 does not accept, or coming back as something Rust
cannot deserialize**:

| | |
|---|---|
| **G-36** | `INTEGER` → JS number → Rust `bool`. A Wasm trap, unloggable |
| **G-38** | Rust `i64` → JS BigInt → D1 bind. Refused, 23 binds |
| **G-39** | Rust `i64` → `as i32` → JS number. Accepted, silently truncating |

Each was found by accident, by someone looking at something else. Two of
them made the service completely non-functional and survived four
milestones and three releases.

**Nobody can currently say what this boundary does.** That is the defect
this subject addresses — not any individual type, but the absence of a
description. A boundary a project cannot describe is one it will keep
tripping over.

## What this subject is not

- **Not a hunt for more bugs.** If it finds them, good, but the
  deliverable is the description.
- **Not a refactor.** The only code change in scope is G-39's thirteen
  `as i32` casts in `db/migration.rs`, adopting 07c's helper.
- **Not open-ended.** §Scope bounds it. If the work grows past that,
  stop and report rather than following it.

## Scope

**Only types that cross the boundary today, or plausibly will.** Not every
Rust type.

### Bind direction — Rust → D1

| Type | Status |
|---|---|
| `&str` / `String` | In use everywhere |
| `i64` | **Characterised by G-38** — BigInt, refused; helper converts |
| `i32` and smaller | In use via `as i32` casts (G-39) |
| `bool` | In use via `as i32` |
| `JsValue::NULL` | In use for every `Option::None` |
| `u64` | **Unexamined** — likely BigInt, same as `i64` |
| `f64` | **Unexamined** — no `REAL` column exists yet; `response_time_ms` is a plausible future one |
| `Vec<u8>` / blob | **Unexamined** — no `BLOB` column exists yet |

### Read direction — D1 → Rust

| SQLite storage class | Arrives as | Deserializes into |
|---|---|---|
| `INTEGER` | **Characterised by G-36** — JS number (f64) | `i64` yes; `bool` no, needs the visitor |
| `TEXT` | In use | `String`, `Option<String>` |
| `NULL` | In use | `Option<T>` |
| `REAL` | **Unexamined** | ? |
| `BLOB` | **Unexamined** | ? |
| `INTEGER` beyond 2^53 | **Unexamined** — and D1 *can* store one even though we now refuse to bind one | ? |

**That last row is the one I would look at first.** `i64_to_d1` refuses to
*write* beyond 2^53. Nothing stops a value arriving there by another route
— a hand-written migration, a direct `wrangler d1 execute`, or a future
`SUM()`/`COUNT()` in a query. What happens on the way back is unknown, and
"we don't write them" is not the same as "they cannot exist."

## Build

### Step 1 — exercise every cell, report before writing anything else

For each cell marked **Unexamined**, and each already-characterised one as
a control, **run it against `wrangler dev --local`** and record what
actually happens. Not what the crate docs say; what the runtime does.

Controls matter: if `i64`-bind does not reproduce G-38's `D1_TYPE_ERROR`
on your harness, your harness is wrong and every other result is suspect.

**Report the table before writing the reference document.** If a cell's
answer is surprising, I would rather rule on it than have it written up.

### Step 2 — `docs/src/d1-type-boundary.md`

The reference nobody could write before. For each type: what it becomes,
whether D1 accepts it, what comes back, and **which helper or cast the
codebase uses**, so the next person adding a column has one page to read
instead of three gap entries.

State the evidence for each row — *"confirmed against the local D1
runtime, 2026-08-xx"* — and mark anything argued rather than run. A row
that says "presumably" is more useful than a row that quietly guesses,
and this project has been bitten by the difference.

Link it from `docs/src/SUMMARY.md` and `docs/src/architecture.md`.

### Step 3 — close G-39

Replace `db/migration.rs`'s thirteen `as i32` casts with 07c's
`i64_to_d1`/`opt_i64_to_d1`. Mechanical, and it makes the codebase convert
integers for D1 by exactly one rule instead of two.

### Step 4 — a test that goes red if an answer changes

The reference document is a snapshot of another system's behaviour. It
will rot silently unless something checks it.

Add a wasm test asserting the load-bearing answers — at minimum: `i64`
binds are refused without the helper, an `INTEGER` column arrives as a
number and not a boolean, and `NULL` arrives as `None`. **If a future
`worker` or `wrangler` version changes any of those, this test fails and
the document gets updated.** Without it, §2 is a document that was true
once.

### Do not

- **Do not fix anything you find beyond G-39.** Report it. A subject that
  audits and repairs is two subjects on one branch, and this one's value
  is in the description being trustworthy.
- **Do not test types nothing plausibly uses.** `i128`, `char`, nested
  enums — out of scope. If you think one belongs, say so first.
- **Do not touch real Cloudflare infrastructure.** Standing rule 7.
- **Do not put `noye-core` tests in `noye-core`** — its wasm test binary
  cannot load (**G-37**). `noye-shared`, as 07b established.

## Verify

| # | Test | Type |
|---|---|---|
| T-203 | Every cell in §Scope's two tables has a recorded outcome — none silently dropped | guard |
| T-204 | Each row of `d1-type-boundary.md` names its evidence and its date, and anything argued rather than run says so | **guard — critical** |
| T-205 | The three controls reproduce G-36, G-38 and G-39's known behaviour — a harness that cannot reproduce a known defect cannot be trusted on an unknown one | **guard — critical** |
| T-206 | `db/migration.rs` binds no integer by `as` cast; `grep` finds none | **must fail first** |
| T-207 | The Step 4 regression test fails when the helper is removed — proven, not assumed | **must fail first** |

**T-205 is the one that makes the rest worth reading.** Every "unexamined"
answer in this subject is only as good as the harness that produced it,
and the only way to know the harness works is to point it at something
whose answer is already known.

## Done

- The §Scope tables fully populated, reported and ruled on
- `docs/src/d1-type-boundary.md` written and linked
- G-39 struck; `db/migration.rs` converts by one rule
- The Step 4 test in CI, proven to go red

## Escalate

- **A control that does not reproduce** → architect, immediately. Stop;
  the harness is wrong.
- **Any cell whose answer implies a live defect** → architect, before
  writing it up. That is a fourth instance and it changes what this
  subject concludes.
- **Scope growing past §Scope** → architect. This subject is bounded on
  purpose.
