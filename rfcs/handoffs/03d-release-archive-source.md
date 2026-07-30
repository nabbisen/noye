# 03d — The release archive is the tagged commit, not the working directory

**Milestone** M0 (release artifact) · **Closes** G-34 · **Satisfies** PRQ-14
**Branch** `fix/03d-archive-source` · **Depends on** nothing
**Governing artifact** — Gap **G-34** (`docs/src/requirements.md` §11)
**Blocks** distributing any release archive. Does **not** block tagging.

## The defect

`package.sh` builds the archive with `tar … .` over the **working
directory**, excluding only `target/`, `Cargo.lock`, `dist/` and
`.git/`. Everything else present on disk ships, tracked or not.

Measured against `4d5893b` on 2026-07-29 — 300 entries, 1.9 MB:

```
./.git-exclude/          54 paths — reviewed/, review-request/, roles/,
                         tasks/, specs/ (incl. a 1.06 MB PDF and the
                         214 KB mockup bundle), tmp/ (a CI log archive)
./.claude/settings.local.json
./.vscode/
```

Two distinct problems:

1. **It publishes the internal working directory.** `.git-exclude/` is
   excluded from *git*; `package.sh` knows nothing about that. A
   distributed archive would carry the entire review trail and local
   tooling configuration.
2. **It is neither tied to the tag nor reproducible.** It captures
   whatever is on disk, so a `0.28.0` archive could be built from a dirty
   tree with nothing to indicate it, and two people would produce
   different archives under the same filename.

The exclude list was correct for the tree `package.sh` was written
against and rotted silently as the tree grew — `.git-exclude/`,
`.claude/`, `scripts/` and `docs/book/` all postdate it. Same shape as
G-32 and G-33: a mechanism that looked configured and was never observed
doing its job.

## Build

Replace the working-tree `tar` with an archive of the **tagged commit**:

```bash
git archive --format=tar.gz --prefix='' "${TAG}" -o "${ARCHIVE}"
```

Only tracked files, exactly the tagged commit, reproducible by anyone
holding the tag. No exclude list to maintain, so nothing left to rot.

Keep unchanged: the `noye-project-v<version>.tar.gz` filename (the `v` is
correct here), the flat layout from subject 03a, the version read from
`cargo metadata`, and the `noye-README-v<version>.md` companion.

The script must **refuse to build from an untagged or dirty state**
rather than silently producing something unreproducible. Which tag to
archive should be explicit — derived from the version, or passed in — not
inferred from `HEAD`.

## ⛔ Decision D-5 must be answered first

`Cargo.lock` is **tracked**, so `git archive` includes it. Today's
`package.sh` excludes it, per DEC-006 — a rule that was applied but never
ratified, and which the parallel UI mockup reversed.

The two are now coupled: `git archive` cannot exclude a tracked file
without extra machinery, so the choice has to be made rather than
inherited.

**Stop and report** for the decision before implementing. Do not pick a
default.

## Non-change scope

`package.sh`'s Japanese header comment (CON-09, subject 34), the archive
filename, the flat layout, and the version source.

## Verify

| # | Test | Type |
|---|---|---|
| T-171 | The archive contains no path under `.git-exclude/`, `.claude/`, `.vscode/`, `target/` or `docs/book/` | **must fail first** |
| T-172 | Every path in the archive is tracked at the archived tag — compare against `git ls-tree -r --name-only <tag>` | **must fail first** |
| T-173 | Building twice from the same tag produces byte-identical archives | **must fail first** |
| T-174 | Building from a dirty or untagged tree is refused, not silently produced | **must fail first** |
| T-175 | The archive still unpacks flat, and the filename still carries the version | guard — subject 03a's property must not regress |
| T-176 | `Cargo.lock` presence matches whatever D-5 decided | guard |

**T-172 is the one that cannot rot.** Asserting the archive equals
`git ls-tree` at the tag needs no maintained list, so a future directory
cannot silently join the archive the way `.git-exclude/` did.

## Required documentation updates

- `docs/src/requirements.md` — PRQ-14 → `Implemented`; G-34 struck
- `docs/src/decision-log.md` — D-5 recorded with its rationale and
  re-evaluation criteria
- `docs/src/deployment.md` or `development.md` — if either documents
  producing a release archive, correct the procedure
- `CHANGELOG.md`

## Done

- All six tests pass; four baseline failures captured
- `rfcs/handoffs/evidence/subject-03d-tests.log` records the entry count
  and top-level listing of a produced archive, before and after

## Escalate

| Situation | Do |
|---|---|
| D-5 unanswered | Stop. Do not choose a `Cargo.lock` default |
| `git archive` cannot produce the flat layout subject 03a established | Report — the two requirements would then conflict and that is a design question |
| Any already-distributed archive is found to contain `.git-exclude/` | **Report immediately.** That is a disclosure question, not a packaging one |
