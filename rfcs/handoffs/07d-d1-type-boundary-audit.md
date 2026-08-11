# 07d — Characterise the D1 type boundary by exercising it

**Milestone** M1.1 · **Closes** G-39, **G-40** · **Produces** `docs/src/d1-type-boundary.md`
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

> **Its central fact, settled by Step 1 (2026-08-11):** integers cross
> this boundary exactly **only within ±2^53, in both directions**
> (**DEC-023**). D1 surfaces every numeric column as a JS Number, so a
> larger `INTEGER` is imprecise before Rust sees it — `i64::MAX` reads
> back as `9.223372036854776e+18`. Writes already enforce the limit
> (`i64_to_d1`); reads cannot recover a violation, only report it
> (**G-41**).
>
> **Build the document around that**, not as a footnote in the
> read-direction table. It is a constraint on the domain model: any
> future schema or query must stay inside it, and the answer for a value
> that genuinely cannot is `TEXT` with explicit parsing — never a
> cleverer deserializer, because the loss happens before Rust is
> involved.

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

### Step 5 — the delete-failure path (DEC-022)

`run_cleanup` archives a batch, then deletes it. Subject 07a confirmed the
**archive**-failure path in both halves. The **delete**-failure path is
untested: if the delete fails after a successful archive, the pass aborts
with those rows archived but present, and the next pass re-archives them.

That behaviour is **correct** — duplication is the accepted cost of never
losing a record (**DEC-022**) — but it has never been observed, and
DR-LIF-07 asserted the opposite until 2026-08-08.

It needs D1 fault injection rather than a missing binding, which is why it
lands here: you will already have the boundary harness up. **Confirm the
record ends up in two archive objects and is deleted exactly once**, and
that nothing is lost. If it turns out a record can be lost, that is an
escalation, not a finding to write up.

### Step 6 — a guard for nested `.git-exclude/` — **reinstated**

> **Struck 2026-08-11, reinstated the same day.** The strike said
> `.gitignore:53`'s root-anchored `/.git-exclude/` already makes a nested
> one visible. **The pattern is anchored and it does not help**, because
> the reviewer tested the wrong path:
>
> ```
> git check-ignore crates/core/.git-exclude/x
>   → NOT ignored                            ← what was tested
> git check-ignore crates/core/.git-exclude/tmp/s/.wrangler/state/v3/d1/db.sqlite
>   → .gitignore:38:.wrangler/               ← what --persist-to creates
> ```
>
> `.wrangler/` at line 38 is **deliberately unanchored** — wrangler
> legitimately creates it in `crates/core` and `crates/gateway` — so a
> nested `.git-exclude/` containing only wrangler state is **entirely
> invisible to `git status`**. Reproduced. The three earlier catches were
> runs that also created seed SQL or logs; the fourth created only state
> and was silent. Found by the dev team on subjects 08–10.

**No `.gitignore` rule can fix this**, because none can distinguish
wrangler state where it belongs from wrangler state inside a directory
that should not exist. **Add a check instead**: any `.git-exclude`
directory outside the repository root is an error. Cheap, and it looks at
the thing that is actually wrong rather than at its contents.

Fold it into an existing gate rather than adding a script.

### Step 6 — diagnose G-40: the gateway's 13 crypto tests

**Folded in on the owner's approval, 2026-08-11.** Be clear about why:
this is **not** the same defect class as G-36/G-38/G-39. Those are *type*
boundary defects. G-40 is a question about what the JS **runtime**
provides under the test harness. The reason to do it here is practical —
you will already have the wasm harness up and Node's behaviour in your
head — not conceptual. If it turns out bigger than that, split it.

`cargo test -p noye-gateway --target wasm32-unknown-unknown` panics at
`auth/crypto/digest.rs:94`. Thirteen tests across four modules — SHA-256,
random generation, base64url, JWT verification — the primitives beneath
the audit hash chain, CSRF tokens, session handling and OIDC.

**Nobody knows whether the code or the harness is at fault**, and that is
the entire point of the entry. `.cargo/config.toml` documents the command
as though it works.

1. **Classify before fixing.** Environmental (Web Crypto unavailable or
   differently shaped under `run_in_node_experimental`) or a genuine
   defect in the primitives?
2. **If environmental** — fix the harness, then **add `noye-gateway` to
   the `wasm-tests` CI job**, which excludes it today precisely because a
   job that is red on arrival gets ignored.
3. **If a defect in the primitives — stop and report.** That is a
   security finding about SHA-256, randomness or JWT verification, and it
   is not this subject's to fix quietly.

**Do not delete or `#[ignore]` a failing crypto test to make the job
green.** If one cannot be made to pass, leave it failing with the reason
recorded — a red test that is understood is worth more than a green suite
that is not.

## Verify

| # | Test | Type |
|---|---|---|
| T-203 | Every cell in §Scope's two tables has a recorded outcome — none silently dropped | guard |
| T-204 | Each row of `d1-type-boundary.md` names its evidence and its date, and anything argued rather than run says so | **guard — critical** |
| T-205 | The three controls reproduce G-36, G-38 and G-39's known behaviour — a harness that cannot reproduce a known defect cannot be trusted on an unknown one | **guard — critical** |
| T-206 | `db/migration.rs` binds no **`i64`/`Option<i64>`** by `as` cast — six sites. *(Restated 2026-08-11: originally "no integer by `as` cast; grep finds none". Seven of the thirteen casts are `bool`, which has no truncation risk and uses the same pattern unflagged in three other modules. Converting those would route a `bool` through an `i64` helper for no gain. The reviewer's G-39 text called all thirteen `i64`; the dev team checked each field's type.)* | **must fail first** |
| T-207 | The Step 4 regression test fails when the helper is removed — proven, not assumed | **must fail first** |
| T-208 | With the **delete** forced to fail after a successful archive, no record is lost; the record appears in two archive objects across the two passes and is deleted exactly once (DEC-022) | **guard — critical** |
| T-209 | A `.git-exclude/` directory anywhere but the repository root fails a gate — **not** by `.gitignore`, which cannot see one containing only `.wrangler/` state | **must fail first** |
| T-210 | G-40 is classified as environmental or a primitive defect, with the evidence for the classification — not a guess | **guard — critical** |
| T-211 | `noye-gateway`'s 13 WASM tests pass, or each still-failing one has a recorded reason and is neither deleted nor `#[ignore]`d | guard |
| T-212 | `noye-gateway` is in the `wasm-tests` CI job, and the job goes **red** when a crypto test fails — proven by breaking one | **must fail first** |

**T-205 is the one that makes the rest worth reading.** Every "unexamined"
answer in this subject is only as good as the harness that produced it,
and the only way to know the harness works is to point it at something
whose answer is already known.

## Done

- The §Scope tables fully populated, reported and ruled on
- `docs/src/d1-type-boundary.md` written and linked
- G-39 struck; `db/migration.rs` converts by one rule
- **G-40 struck or escalated** — the 13 gateway tests classified, and `noye-gateway` in the `wasm-tests` job if they pass
- The Step 4 test in CI, proven to go red

## Escalate

- **A control that does not reproduce** → architect, immediately. Stop;
  the harness is wrong.
- **Any cell whose answer implies a live defect** → architect, before
  writing it up. That is a fourth instance and it changes what this
  subject concludes.
- **Scope growing past §Scope** → architect. This subject is bounded on
  purpose.
- **G-40 turning out to be a defect in a cryptographic primitive** →
  architect, immediately, before any fix. It stops being this subject's
  work the moment it is one.
