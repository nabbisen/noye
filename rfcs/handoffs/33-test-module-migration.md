# 33 — Tests move to sibling modules

**Milestone** M5 · **Closes** G-23 · **Satisfies** PRQ-05
**Branch** `chore/33-test-modules` · **Depends on** nothing — may run in parallel
**Governing artifact** — Gap **G-23** (§11)

## The defect

40 implementation files carry an inline `#[cfg(test)] mod tests`. **Zero
sibling `tests.rs` files exist.** The project rule marks the inline form
explicitly as ❌ Bad and requires `src/some_mod/tests.rs`.

The rule is not partially followed. It is not followed at all — while
`docs/src/requirements.md` marked PRQ-05 `Implemented`.

## Build

Move each into `src/<mod>/tests.rs`. Where a file exceeds the line-count
guidance, split into `src/<mod>/tests/`.

Mechanical and low-risk. This is the one M5 subject that can be done in
slices without leaving the tree inconsistent — take a few modules at a
time.

### ⛔ No test behaviour changes

**If a test needs editing to keep passing after a move, stop and
report.** Location should not change behaviour, and a test that depends
on being inside the implementation module is telling you something about
its coupling.

## Verify

| # | Test | Type |
|---|---|---|
| T-154 | The full suite passes with an identical test count **per crate** before and after each move | **guard — critical** |
| T-155 | No implementation file contains `#[cfg(test)] mod tests` | **must fail first** |
| T-156 | Every test module lives in `src/<mod>/tests.rs` or `src/<mod>/tests/` | **must fail first** |

**T-154 compares per crate, not just the total.** A test silently dropped
in one module and added in another would cancel out.

## Done

- All three tests pass
- For the first time in the project's history, every test file complies
  with PRQ-05
- `docs/src/requirements.md`: PRQ-05 → `Implemented`, G-23 struck

## Escalate

A test needing modification to survive a move → requirements architect.
