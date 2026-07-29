# 03b — The CI dependency scan actually runs

**Milestone** M0 · **Closes** G-32 · **Satisfies** NFR-SEC-10, NFR-QA-06
**Branch** `fix/03b-ci-audit` · **Depends on** nothing — may run alongside 03a
**Governing artifact** — Gap **G-32** (`docs/src/requirements.md` §11)
**Blocks** the v0.28.0 release.

## The defect

`.github/workflows/ci.yml:173`:

```yaml
run: cargo audit --locked
```

`cargo audit` has no `--locked` flag — it reads `Cargo.lock` directly.
cargo-audit 0.22.2 rejects the invocation before scanning anything:

```
error: unexpected argument '--locked' found
  tip: a similar argument exists: '--no-yanked'
```

**So the dependency-vulnerability scan has never run.** NFR-SEC-10
requires a scan "on every change and on a recurring schedule" and is
marked `Implemented`.

### The evidence that this is inert, not merely mis-typed

RUSTSEC-2026-0190 (`anyhow`, unsound `Error::downcast_mut`) was published
**2026-06-25**. It surfaced on **2026-07-28**, when an implementer ran
`cargo audit` by hand for the first time.

A working scan — every push, plus a weekly cron — had a month and several
scheduled runs to catch it. It caught nothing, because the command exits
before scanning. That is a control reporting success while doing nothing,
which is the defect class this whole remediation programme exists to
close.

## Build

1. **`.github/workflows/ci.yml:173`** → `run: cargo audit`
2. **Confirm it runs in a real GitHub Actions run**, not only locally.
   The job installs cargo-audit; confirm the installed version accepts
   the invocation and that the job's pass/fail is real.
3. **While you are in a real Actions run**, settle the question you
   raised in review request `003` and I never answered: the `migrations`
   job uses `fetch-depth: 0` so it can read tag `0.1.0` for the Class A
   fixture. Confirm that works on a real runner — a shallow clone would
   make T-01a silently unable to build its fixture, which would look
   like a pass.

## Non-change scope

Do not touch: the other CI jobs, `.cargo/audit.toml`, `Cargo.lock`,
`package.sh`, or any dependency version. This subject changes one line of
CI configuration and verifies it.

## Prohibited

- **Do not add a suppression to `.cargo/audit.toml` to make the job
  green.** Suppression is for an advisory with no upstream fix, carrying
  a written rationale and a re-evaluation trigger — SEC-001 is the
  precedent. A suppression added to quiet a newly-working gate would
  defeat the purpose of fixing it.
- **Do not make the job non-blocking** to get a green run. If the
  now-working scan finds something, that is the gate doing its job:
  report it.
- Do not "verify" by reasoning about the YAML. This defect existed
  because nobody ran it.

## Security constraint

This is a security control, not a build convenience. If the first real
run surfaces advisories, **stop and report** rather than deciding how to
handle them — triage is the reviewer's, and a decision to accept residual
risk is the human owner's.

## Verify

| # | Test | Type |
|---|---|---|
| T-164 | The CI audit invocation is accepted by the installed cargo-audit — the job reaches a scan rather than an argument error | **must fail first** |
| T-165 | The job runs to completion in a real Actions run and its result reflects an actual scan (crate count reported) | **must fail first** |
| T-166 | The gate **detects** a vulnerable dependency — verified against a deliberately vulnerable lockfile entry in a scratch branch, not by trusting a clean exit | **must fail first** |
| T-167 | The `migrations` job's `fetch-depth: 0` resolves tag `0.1.0` on a real runner, so T-01a's Class A fixture is genuinely built | guard |

**T-166 is the one that matters.** A gate that exits 0 because it
scanned nothing looks identical to a gate that exits 0 because the tree
is clean — that is precisely how this went unnoticed for a month. Prove
it fails on something before trusting it to pass on nothing.

Use a scratch branch, confirm the job fails, then discard the branch.
Record the failing run's URL or output in the evidence.

## Required documentation updates

- `docs/src/requirements.md` — NFR-SEC-10 → `Implemented`; G-32 struck,
  not deleted
- `docs/src/development.md` — if it documents the CI gate set, correct
  the invocation there too
- `CHANGELOG.md`

## Done

- All four tests pass; three baseline failures captured
- The first genuinely-executed scan's output recorded in
  `rfcs/handoffs/evidence/subject-03b-tests.log`, including the crate
  count, so a future reader can tell a real scan from an aborted one

## Escalate

| Situation | Do |
|---|---|
| The first real scan surfaces advisories | **Stop and report.** Do not triage, suppress, or update dependencies without review |
| `fetch-depth: 0` does not give the runner tag `0.1.0` | Report — T-01a's fixture is affected, and subject 01 is already merged |
| The installed cargo-audit differs from local | Report the version; the gate must work with what CI actually installs, not what you have |
