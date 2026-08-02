# 36 — Release rehearsal and v1.0.0

**Milestone** M5 · **Branch** `release/1.0.0` · **Depends on** subjects 29–35
**Last subject, by definition.**
**Governing artifact** — **Release governance** — Phase 7 release-candidate preparation · closes **D-5**, **DEC-017**, and the live-confirmation residual on **DR-LIF-06**, **DR-LIF-07**, **FR-AUD-06**

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

> **Subject 07a triages this list first (2026-08-02).** Six live-confirmation
> obligations had accumulated here, and standing rule 7 means no agent may
> execute any of them — this subject was, as written, unassignable. 07a
> determines which are confirmable against the **local D1 runtime**
> (a real D1 runtime, not a SQLite stand-in — subject 06's Step 0 proved
> the difference), executes those, and packages the rest as one prepared
> sitting for the owner. **Whatever 07a closes is removed from here.**
> Read 07a's report before working this subject; the list below may be
> shorter than it looks.

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
4. **Observe a retention pass — the one live confirmation the whole
   retention module has never had.** No retention behaviour has ever been
   executed end to end, for any data class, in any environment: every
   claim about it rests on unit tests over the deciding functions plus
   control-flow argument. That is acceptable in a development environment
   and is not acceptable at 1.0. **Confirm all four here, in one
   rehearsal, not one at a time:**
   - **DR-LIF-06** — with more eligible `check_results` than one batch
     holds, the count archived equals the count deleted, and every deleted
     row is present in an R2 archive object.
   - **DR-LIF-07** — *fault injection.* Force the R2 write to fail; no row
     is deleted. Then let a later pass succeed; every eligible row is
     archived and deleted exactly once. This is the one that needs a fault,
     which is why it alone reads `Partial` today.
   - **FR-AUD-06** — **reinsert an `audit_logs` policy row by hand**
     (`INSERT INTO retention_policies VALUES ('audit_logs', 365, 1, NULL)`),
     seed an audit row older than the cutoff, run a full pass, and confirm
     zero audit rows were deleted and the chain still verifies. This is
     T-16 as written; subject 04 could only reach it by proxy, and the
     ruling accepting that proxy
     (`.git-exclude/reviewed/023-audit-subject-04.md` §4) named this step
     as where it gets closed properly.
   - **DEC-017** — as measured in step 3 above.
   - **DEC-020's per-write cost.** `current_head_hash` walks the whole
     `audit_logs` table on every audit write. Measure it against live D1
     at a realistic row count and record the number. If it is material,
     the first mitigation is a recursive CTE pushing the walk into D1 —
     **confirm D1 supports one before specifying it**; that has not been
     verified in any development environment.
5. **Threat model refresh.** Subjects 06, 07, 08 and 29 all changed data
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
test across subjects 01–35.

**Cite reproduction, not logs.** For each test, record the parent commit
and the test name — enough that any reader can run it themselves:

```
T-16   git checkout <parent-sha> && cargo test retention_keeps_audit_rows
       expected: fails (pre-fix)   confirmed: <date>
```

Do **not** archive or quote the baseline log files. A log is something a
reader has to trust; a command is something they can run. The evidence
is reproducible from git indefinitely, which makes the citation the
stronger artifact and the log merely a convenience at review time.

This is the direct answer to the finding that started this work — an
evidence file asserting an exit code for a command that had never been
run. The fix is not a better-curated log. It is making the claim
independently checkable.

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
