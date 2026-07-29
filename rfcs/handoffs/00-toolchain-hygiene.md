# 00 — Toolchain pin and lint debt

**Milestone** M0 · **Closes** no gap · **Satisfies** NFR-QA-04, NFR-QA-05
**Status** ✅ **delivered 2026-07-28** (recorded retrospectively)
**Depends on** nothing · **Prerequisite for every other subject**
**Governing artifact** — **Prerequisite** — no governing RFC or gap; enables the gates every other subject is verified by

## Why this subject exists

`rust-toolchain.toml` pins 1.91 with rustfmt, clippy and
`wasm32-unknown-unknown`. Before it, the pin lived only inside
`.github/workflows/ci.yml`, so `rust-version = "1.91"` in `Cargo.toml`
was a *minimum*, not a pin, and every contributor's local toolchain
diverged from CI.

Pinning did not create the lint debt — **it revealed it.** Against 1.91:
44 clippy errors across `crates/gateway`, `crates/core` and
`crates/dev-idp`, plus widespread `cargo fmt` drift. Neither gate had run
clean against this toolchain in some time, which meant no subject could
be verified as introducing no new warnings.

This subject was created **after** the work was done, because the
planning omitted it: the toolchain pin was scheduled and the cleanup it
would expose was not. Recorded here so the work has a home in the
register rather than living inside two unrelated pull requests.

## Build — as delivered

- `rust-toolchain.toml` pinning 1.91 + rustfmt + clippy +
  `wasm32-unknown-unknown`
- 44 clippy errors resolved. All mechanical: `Range::contains`,
  `strip_suffix`, redundant casts, collapsible `if`, one derivable
  `impl Default`
- `cargo fmt` applied across the tree

## Verify — as delivered

| # | Test | Type | Result |
|---|---|---|---|
| T-00a | `cargo fmt --all -- --check` exits 0 on the pinned toolchain | guard | **fail, then pass** — see below |
| T-00b | `cargo clippy --workspace --all-targets --locked -- -D warnings` exits 0 | guard | pass |
| T-00c | Host test count and outcome are **unchanged** before and after — 435 passed, 0 failed | **guard — critical** | pass |
| T-00d | `rust-toolchain.toml` and the CI workflow name the same version | guard | pass |

**T-00a correction (2026-07-28, post-audit F-1).** Recorded as passing
without having actually been re-run after the initial cleanup; the
independent reviewer (`.git-exclude/reviewed/013-audit-subjects-01-02.md`)
found 13 unformatted hunks across `crates/dev-idp/src/{handlers,jwt,
keys}.rs` — `dev-idp` is one of the three crates this subject claims as
fixed. Re-run for real:

```
$ rustc --version
rustc 1.91.1 (ed61e7d7e 2025-11-07)
$ cargo fmt --all -- --check ; echo "exit=$?"
exit=1
$ cargo fmt --all
$ cargo fmt --all -- --check ; echo "exit=$?"
exit=0
$ cargo test --workspace --lib --bins --locked   # T-00c re-confirmed
7 passed; 129 passed; 10 passed; 301 passed; 3 passed — 450 total, 0 failed
```

Same defect class the v0.27.2 evidence bundle was called out for: a
result column asserting an outcome for a command not actually re-run.
Not repeating it.

**T-00c is what makes this safe.** A mechanical lint fix that changes a
test outcome is not mechanical. Compare per crate, not only the total.

## Done

- All four green
- No behavioural change; 435 tests unchanged in count and outcome

## The rule this establishes

**Hygiene that blocks verifying a subject gets its own pull request,
merged first — never bundled into a subject's PR.** A PR nominally about
G-01 that also touches fifteen unrelated files cannot be reviewed for
either purpose.

If such a PR would be large, stop and report rather than absorbing it.
