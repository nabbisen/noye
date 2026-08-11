# 07e — `sha256()` can never succeed, so OIDC login cannot start

**Milestone** M1.1 · **Closes** G-42 · **Finishes** G-40's CI half
**Branch** `fix/07e-sha256` · **Depends on** 07d
**Governing artifact** — Gap **G-42** (§11)

## The defect

`crates/gateway/src/auth/crypto/digest.rs:30`:

```rust
let array: Uint8Array = result
    .dyn_into()
    .map_err(|_| "digest did not return ArrayBuffer/Uint8Array".to_string())?;
// The result is an ArrayBuffer, so wrap it in a Uint8Array.
let wrapped = Uint8Array::new(&array);
```

`subtle.digest()` resolves to an **`ArrayBuffer`**. `dyn_into::<Uint8Array>()`
is an `instanceof` check, and an `ArrayBuffer` is never `instanceof
Uint8Array`. **Line 30 cannot succeed in any conforming JS engine.**

The comment on line 34 names the correct fix and is unreachable, because
the annotation two lines above it is wrong. Whoever wrote this had the
right idea and typed the wrong intermediate type.

**One call site**, and it is the front door: `auth/oidc.rs:164`, the PKCE
S256 code challenge, on **every login initiation**.

### It fails closed

`sha256()` returns `Result`; `oidc.rs:166` maps it to `Error::RustError`.
A login attempt **fails with an error**. It does not produce a weak
challenge, a predictable verifier, or a bypass.

**This is an outage, not a vulnerability**, and it should be described
that way everywhere — including in the changelog, which publishes
verbatim. Nothing is less secure than intended; nobody can log in.

## ⛔ Step 0 — observe it on workerd first

The failure has been seen under **Node** (`cargo test --target
wasm32-unknown-unknown`). It has **not** been seen on the runtime
Cloudflare actually deploys.

**`wrangler dev --local` runs `workerd`** — the open-source Workers
runtime, present as a wrangler dependency — so this is answerable
locally and inside standing rule 7. Stand up a temporary route that
calls `crypto::sha256(b"abc")` and record what workerd does.

Expected: the same failure, because it is a language-level type fact
rather than a runtime API difference.

**If it succeeds on workerd, stop and report.** That would mean workerd's
`crypto.subtle` returns something `instanceof Uint8Array` — non-spec —
and it would change G-42 from "login has never worked" to "login works in
production and only the tests are broken," which is a completely
different finding.

**Record which runtime each observation came from.** Node and workerd are
different answers to the same question and this subject depends on
telling them apart.

## Build

1. **Fix the annotation.** The intermediate is an `ArrayBuffer`; wrap it.
   The existing comment already describes this.
2. **Make the error message honest.** `"digest did not return
   ArrayBuffer/Uint8Array"` described a condition that could not be
   distinguished from the bug itself. Whatever replaces it should name
   what was actually received.

### Do not

- **Do not change `oidc.rs`.** The PKCE construction is correct; only the
  digest helper is broken.
- **Do not widen into the other crypto modules.** `base64url`,
  `jwt_verify` and `random` pass — 9 of 13 — and are out of scope.
- **Do not "fix" the failing tests.** They are correct and have been
  correctly failing. The code is what changes.

## Verify

| # | Test | Type |
|---|---|---|
| T-213 | `sha256()` returns the FIPS 180-4 vector for `"abc"` — the existing test, now passing | **must fail first** |
| T-214 | The same, **observed under workerd** via `wrangler dev --local`, not only under Node | **guard — critical** |
| T-215 | `oidc.rs`'s login-initiation path produces a PKCE challenge end to end, rather than a 500 | **must fail first** |
| T-216 | `noye-gateway` is in the `wasm-tests` CI job, and the job goes **red** when a crypto test fails — proven by breaking one | **must fail first** |
| T-217 | All 13 gateway WASM tests pass; none deleted, none `#[ignore]`d | guard |

**T-214 is the one that settles it.** Node's answer is suggestive;
workerd's is the one that describes production. Every other test here is
worth less without it.

**T-216 finishes what G-40 started.** The job excluded `noye-gateway`
because it was red on arrival; once it is green, it goes in — with the
same red-proof requirement every other gate in this project carries.

## Done

- All five tests pass; T-213's, T-215's and T-216's baselines captured
- `docs/src/requirements.md`: G-42 struck, mechanism recorded
- `CHANGELOG.md` — **and describe it as an outage, not a vulnerability**
- `docs/src/d1-type-boundary.md` gains a line recording that
  `wrangler dev --local` is workerd, so runtime questions are locally
  answerable — a standing capability nobody had written down

## Escalate

- **`sha256()` succeeding on workerd** in Step 0 → architect, before the
  fix. It changes what this gap is.
- **Any of the other nine gateway tests failing** once the job is wired
  up → architect. They pass today; a new failure is a new finding.
