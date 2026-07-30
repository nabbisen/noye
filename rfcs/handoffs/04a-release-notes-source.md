# 04a — Release notes are the curated changelog

**Milestone** M1 · **Closes** G-35 · **Satisfies** PRQ-15
**Branch** `fix/04a-release-notes-source` · **Depends on** 03d
**Governing artifact** — Gap **G-35** (§11), **PRQ-15**

**Work this before 05.** It is numbered `04a` rather than `37` because
subjects are worked in numeric order and this one belongs between 04 and
05 — the next free number belongs to a later subject. Same rule as test
numbering (README § Test numbering), same reason.

## The defect

`.github/workflows/release.yml:50` publishes with:

```
gh release create "${TAG}" dist/* --title "${TAG}" --generate-notes
```

`--generate-notes` produces GitHub's automatic commit and pull-request
summary. `RELEASE.md` § Release notes requires something else entirely:

> Draw from the dated `CHANGELOG.md` entry. State explicitly: anything
> that can break a running deployment or local setup; migration steps an
> operator must take; known issues carried forward, with the subject that
> closes each; rollback — what reverting restores, including any defect
> it reinstates.

A commit list states none of those. The curated changelog is written, and
then not published.

### Why this stops being harmless at 0.28.2

It was harmless for 0.28.1, which changed nothing an operator sees. It
would **not** have been harmless for 0.28.0, whose notes needed to say
*"a published credential is now refused unconditionally; any deployment
still holding the old `GATEWAY_SHARED_TOKEN` will be refused on every
request."* No commit list conveys that.

0.28.2's `[Unreleased]` section already carries two things an operator
needs and a commit list destroys:

- `wrangler d1 migrations apply` is **required**
- **no existing deployment lost an audit row** — the fact that stops an
  operator from going to look for damage that does not exist

This subject exists so those reach the person they were written for.

### The second, larger defect

A release whose changelog section was never written **still publishes**,
with an auto-generated body, and reports success. Nothing goes red. That
is the same shape as G-32 and G-33: a mechanism that appears to be doing
its job and is not, discovered only by someone reading the output.

Fixing the source of the notes without making the missing case fail
would leave that half in place. **Both halves, or neither.**

## Build

### Part 1 — extract, then publish what was extracted

1. **`scripts/changelog-section.sh <version>`** — prints the body of the
   `## [<version>] — <date>` section of `CHANGELOG.md`, up to but
   excluding the next `## [` heading. Requirements:
   - Exits **non-zero with a message on stderr** when no section for
     `<version>` exists.
   - Exits **non-zero** when the section exists but its body is empty or
     whitespace-only — a heading with nothing under it is a changelog
     that was not written.
   - Matches on `## [<version>]` exactly. Do not match a prefix:
     `0.28.1` must not match `## [0.28.10]`.
   - `set -eu`, no `set -o pipefail` reliance on non-bash shells; the
     repository's other scripts are `#!/usr/bin/env bash`.
   - Takes the version as an argument rather than reading `Cargo.toml`,
     so the workflow passes the pushed tag and a human can dry-run any
     version.

2. **`release.yml`** — extract before publishing, and pass the file:
   - Add a step that runs `scripts/changelog-section.sh "${GITHUB_REF_NAME}"
     > notes.md`. Because the workflow uses the default `set -e` shell,
     a non-zero exit **fails the job before any release is created**.
     That is the point: no changelog section, no release.
   - `gh release create … --notes-file notes.md`, replacing
     `--generate-notes`.
   - The already-exists branch takes `gh release edit "${TAG}"
     --notes-file notes.md` alongside its `gh release upload --clobber`,
     so a re-run converges on the same result as a first run rather than
     leaving whatever the first attempt published.

3. **`RELEASE.md`** — add a dry-run step before the tag:
   `bash scripts/changelog-section.sh <version>` prints exactly what will
   be published. The notes become checkable **before** the tag is pushed,
   which is the only point at which they are still cheap to fix. Note
   also that the changelog section is now a release **gate**, not a
   courtesy.

### Part 2 — the deprecated action runtime

The 2026-07-30 run on `main` ([`30591455367`](https://github.com/nabbisen/noye/actions/runs/30591455367))
annotated three jobs:

> Node.js 20 is deprecated. The following actions target Node.js 20 but
> are being forced to run on Node.js 24: `actions/cache@v4`,
> `actions/checkout@v4`.

Bump both, in `.github/workflows/ci.yml` (four `checkout`, three `cache`)
and `.github/workflows/release.yml` (one `checkout`), to the current major
that runs on Node 24 natively.

**Verify by observing the annotation disappear from a real run** — not by
reading the action's release notes. Determining the correct major version
is part of the work; this document deliberately does not name one, because
naming a version I have not confirmed against the runner is exactly the
kind of read-don't-observe claim this project keeps finding.

**This is a separate commit from Part 1**, and it is a deliberate
exception to standing rule 5 (hygiene gets its own pull request). The
judgment: ~8 changed lines, confined to the two workflow files Part 1
already opens, reviewable at a glance. Bundling it does not make either
change harder to review, and a separate pull request for eight lines is
ceremony. If it grows past the workflow files, stop and split it.

## Do not

- **Do not have the workflow write or edit `CHANGELOG.md`.** It reads.
  The changelog is authored by a person as part of the release commit,
  and a workflow that can repair it removes the failure this subject
  exists to create.
- **Do not fall back to `--generate-notes` when extraction fails.** A
  fallback reinstates G-35 and hides it better than the original.
- **Do not reformat existing changelog entries** to suit the extractor.
  If an existing entry does not parse, that is a finding about the
  extractor — report it.
- **Do not touch `package.sh`.** 03d settled the archive; this subject
  changes only what is written beside it.

## Verify

| # | Test | Type |
|---|---|---|
| T-178 | The published body of a scratch release contains a distinctive line from the changelog section verbatim | **must fail first** |
| T-179 | The extractor prints the requested version's section only — not the one above it, not the one below, not the `[Unreleased]` heading | guard |
| T-180 | The extractor exits non-zero, and the workflow job fails with **no release created**, when the tag has no dated changelog section | **must fail first** |
| T-181 | The extractor exits non-zero when the section heading exists but its body is empty | guard |
| T-182 | `0.28.1` does not match `## [0.28.10]` | guard |
| T-183 | Re-running the workflow against an existing release leaves the same notes, not the first attempt's | guard |

**T-178 and T-180 are the ones that matter**, and both need a real run —
follow 03d's method: a scratch tag on a scratch branch, published, read
back with `gh release view --json body`, then tag, branch and release
deleted. 03d proved that method works; do not substitute a local
simulation for it.

T-180 is the half that would otherwise be assumed. Prove the release goes
**red** with the changelog section absent — a workflow that only ever
succeeds is indistinguishable from one that does not check, which is the
whole lesson of G-32 and G-33.

## Done

- All six tests pass; T-178's and T-180's baselines captured against the
  current workflow into `.git-exclude/evidence/baseline-04a.log`
- The Part 2 annotation is confirmed gone from a real run, with the run
  URL in the evidence log
- `docs/src/requirements.md`: PRQ-15 → `Implemented`, G-35 struck
- `CHANGELOG.md` updated — and note that from this subject onward, that
  entry is what ships

## Escalate

- **If `gh release create` cannot fail cleanly after the archive is
  built** — i.e. if extraction can only run after assets exist — stop and
  report rather than reordering the job on your own judgment. The
  ordering constraint (fail before publishing anything) is the
  requirement; how to satisfy it is open.
- **If an existing changelog entry does not parse**, report it as an
  extractor finding. Do not edit released changelog entries to fit —
  their text is what shipped.
