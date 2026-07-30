# 36 — Release rehearsal and v1.0.0

**Milestone** M5 · **Branch** `release/1.0.0` · **Depends on** subjects 29–35
**Last subject, by definition.**
**Governing artifact** — **Release governance** — Phase 7 release-candidate preparation · closes **D-5**, **DEC-017**

## Decision D-5, first

**Does the release archive carry `Cargo.lock`?**

DEC-006 excludes it, for a stated reason — CI wants pinned resolution,
archive recipients want a clean source tree. It was applied but never
ratified, and the parallel UI mockup adopted the opposite convention.

Ratify or change it, record it in `docs/src/decision-log.md` with
re-evaluation criteria, and make `package.sh` match. **This is the last
open decision in the project.**

## Also here

- `crates/core/wrangler.toml.example` runs
  `cargo install -q worker-build && worker-build --release` as its build
  command. Installing a tool during the build makes it non-reproducible
  and network-dependent, against NFR-QA-06's intent. Pin and pre-install.
- Confirm `rust-toolchain.toml` and the CI pin name the same version.
  They must move together.

## The rehearsal

1. **Provision from a clean Cloudflare account** following
   `docs/src/setup.md` alone. **Every undocumented step you have to
   improvise is a documentation defect** — fix the document, not just
   your own run.
2. Create a target, let it be probed, force a failure, confirm the
   notification arrives, confirm the incident opens and resolves, confirm
   the audit chain verifies.
3. **Measure and close DEC-017.** Two numbers, against real D1:
   - the per-statement bound-parameter ceiling — the upper bound on
     `RETENTION_BATCH_SIZE`
   - batches completed per cron invocation before the subrequest budget
     is reached — the practical lower bound

   Record both in the decision log and set the constant from measurement
   rather than from the conservative guess shipped at M0.
3. **Threat model refresh.** Subjects 06, 07, 08 and 29 all changed data
   flows, so this is a refresh rather than a re-verification —
   `docs/src/security-posture.md`.

## Evidence — the deliverable

Into `.git-exclude/evidence/release-1.0.0.log`, with real captured output:

### 1. Full gate set

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check --workspace --locked
cargo test --workspace --lib --bins --locked
cargo check -p noye-gateway --target wasm32-unknown-unknown --locked
cargo check -p noye-core    --target wasm32-unknown-unknown --locked
cargo audit
```
Plus the migration-apply gate from subject 01.

`cargo audit` takes no `--locked` flag — it reads `Cargo.lock` directly.
The CI job and these documents carried `cargo audit --locked` until
2026-07-28, which cargo-audit rejects outright, so the scan never ran
(gap G-32). Confirm the CI job's invocation matches this one.

### 2. The must-fail-first register

**The most important artifact in the project.** Every must-fail-first
test across subjects 01–35, with its baseline failure and its current
pass.

That table is what makes "thirty gaps closed" checkable by someone who
was not here — and it is the direct answer to the finding that started
this work, where a shipped evidence file asserted an exit code for a
command that had never been run.

### 3. Requirement sweep

Confirm no requirement in `docs/src/requirements.md` is marked `Not met`
without an RFC that owns it.

**That is the definition of done for 1.0.** Not "no known defects" —
**nothing unowned.**

## Done

- D-5 recorded
- Rehearsal completed and every improvised step folded back into the docs
- Evidence captured
- Requirement sweep clean
- Tag `1.0.0` — no `v` prefix; tags are bare versions (`0.0.1`, `0.1.0`, `0.27.2`).
  Note the archive filename does carry one, `noye-project-v<version>.tar.gz`,
  from `package.sh`. Tag and artifact conventions differ on purpose.

## Escalate

A requirement still `Not met` with no RFC → **stop.** That is the one
condition that blocks 1.0.
