# 03c — The format, lint and check gates actually run

**Milestone** M0 · **Closes** G-33 · **Satisfies** NFR-QA-04, NFR-QA-05, NFR-QA-06
**Branch** `fix/03c-ci-toolchain` · **Depends on** nothing
**Governing artifact** — Gap **G-33** (`docs/src/requirements.md` §11)
**Blocks** the v0.28.0 release.

## The defect

`.github/workflows/ci.yml:42`:

```yaml
rustup toolchain install 1.91 --profile minimal --component rustfmt clippy
```

`--component` takes a **comma-separated** list. Space-separated, `clippy`
is parsed as a second positional toolchain and rejected:

```
error: invalid value 'clippy' for '[TOOLCHAIN]...': invalid toolchain name: 'clippy'
```

The "Format, lint, check" job fails at its first step. **Format, Clippy
and Cargo check have never run in CI**, from `5de978d` — the 0.27.2
baseline — onward.

NFR-QA-04, NFR-QA-05 and NFR-QA-06 were all marked `Implemented` on the
strength of this job. The properties do currently hold: the commands
pass when run by hand. What does not hold is that anything *enforces*
they keep holding, which is the entire content of those three
requirements.

Only line 42 is affected. Lines 77, 112 and 163 use `--profile minimal`
alone, and the wasm job adds its target with a separate
`rustup target add`, which is correct.

## Build

`.github/workflows/ci.yml:42` → comma-separated:

```yaml
rustup toolchain install 1.91 --profile minimal --component rustfmt,clippy
```

Then confirm in a real Actions run that the job reaches Format, Clippy
and Check, and that all three report.

## Non-change scope

The other three jobs, any gate command, `rust-toolchain.toml`, and the
workflow's triggers or caching. This subject changes one character.

## Prohibited

- Do not "verify" by reading the YAML. All three M0 CI defects — G-21,
  G-32, G-33 — survived because they were checked by reading
  configuration rather than observing a run.
- Do not work around it by installing components in a separate step
  unless the comma form fails on the runner. If it does, report:
  `rust-toolchain.toml` already declares both components, and which
  mechanism should own this is a design question, not an implementation
  choice.

## Verify

| # | Test | Type |
|---|---|---|
| T-168 | The toolchain-install step completes and both `rustfmt` and `clippy` are present | **must fail first** |
| T-169 | Format, Clippy and Cargo check each run to completion and report a result in a real Actions run | **must fail first** |
| T-170 | The job **fails** when a formatting or clippy violation is present — verified on a scratch branch, not inferred | **must fail first** |

**T-170 is the same test as 03b's T-166, for a different gate.** A gate
that exits 0 having never executed looks identical to one that exits 0
on a clean tree. Introduce one violation on a scratch branch, confirm
the job goes red, discard the branch. Record the failing run.

## Required documentation updates

- `docs/src/requirements.md` — NFR-QA-04, NFR-QA-05, NFR-QA-06 →
  `Implemented`; G-33 struck, not deleted
- `docs/src/development.md` — if it describes the CI gate set, confirm
  it matches what now runs
- `CHANGELOG.md`

## Done

- All three tests pass, with the failing scratch run recorded
- `rfcs/handoffs/evidence/subject-03c-tests.log` cites the run directly

## Escalate

| Situation | Do |
|---|---|
| The comma form also fails on the runner | Report — the fix is then a design question about whether `rust-toolchain.toml` or the workflow owns component installation |
| Format or Clippy fails once the job actually runs | **Stop and report.** That would mean the tree has drifted while nothing was watching, and how to handle it is not an implementation choice |
