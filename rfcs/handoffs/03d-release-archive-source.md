# 03d — The release artifact is built by CI from the tag

**Milestone** M0 (release artifact) · **Closes** G-34 · **Satisfies** PRQ-08, PRQ-14
**Branch** `fix/03d-release-workflow` · **Depends on** nothing
**Governing artifact** — Gap **G-34** (`docs/src/requirements.md` §11)
**Blocks** distributing any release archive. Does **not** block tagging.
**No open decisions** — D-5 answered as DEC-019.

## The defect

`package.sh` builds the archive with `tar … .` over the **working
directory**, excluding only `target/`, `Cargo.lock`, `dist/` and
`.git/`. Everything else on disk ships, tracked or not.

Measured against `4d5893b` on 2026-07-29 — 300 entries, 1.9 MB:

```
./.git-exclude/          54 paths — reviewed/, review-request/, roles/,
                         tasks/, specs/ (a 1.06 MB PDF, the 214 KB
                         mockup bundle), tmp/ (a CI log archive)
./.claude/settings.local.json
```

Both are **untracked**, which is why `git archive` excludes them without
an exclude list.

> **Correction, 2026-07-30.** This list originally also named `.vscode/`,
> and T-171 required excluding it. That was wrong: `.vscode/settings.json`
> and `.vscode/extensions.json` are **tracked**, so PRQ-14 and T-172
> require the archive to *include* them, and T-171 contradicted both. The
> error was conflating "was in the old archive" with "should not ship" —
> the old `tar` swept up everything on disk, tracked or not, and only the
> untracked half was the defect. Raised by the implementer, who correctly
> declined to untrack repository content to make a test pass. T-171a now
> asserts the positive case, so nobody closes the gap from the other
> direction.

Three problems, in increasing order of importance:

1. **It publishes the internal working directory.** `.git-exclude/` is
   excluded from *git*; `package.sh` knows nothing about that.
2. **It is not tied to the tag and not reproducible.** It captures
   whatever is on disk, so a `0.28.0` archive could be built from a
   dirty tree with nothing to indicate it.
3. **It runs locally, so nobody observes it.** That is the same
   condition that let G-21, G-32 and G-33 survive from the 0.27.2
   baseline: a mechanism that looks configured and is never watched
   doing its job. A release artifact produced by a human running a
   script has no evidence trail; one produced by a workflow run does.

The exclude list was correct for the tree `package.sh` was written
against and rotted silently as `.git-exclude/`, `.claude/`, `scripts/`
and `docs/book/` appeared.

## Why not GitHub's automatic source archives

GitHub attaches source archives to every release for free. They cannot
be used here — they carry a `<repo>-<tag>/` parent directory:

```
$ git archive --prefix="noye-0.28.0/" 0.27.2 | tar tz | head -1
noye-0.28.0/
```

That is exactly the nested layout PRQ-08 forbids and subject 03a
removed. A custom artifact is therefore required, not merely preferred.

## Build

### 1. A release workflow

`.github/workflows/release.yml`, triggered on a pushed tag matching the
project's bare-version form (`0.28.0`, no `v` — see
`rfcs/handoffs/README.md` § Commit and tag conventions):

- Check out the tag
- Produce the archive by invoking `package.sh` — **one implementation,
  not two.** Do not reimplement the archive logic in YAML; a second
  code path that must agree with the first is how they drift
- Create or update the GitHub Release for the tag and attach the archive
  and the `noye-README-v<version>.md` companion

### 2. `package.sh` archives the tag, not the tree

```bash
git archive --format=tar.gz --prefix='' "${TAG}" -o "${ARCHIVE}"
```

- Only tracked files, exactly the tagged commit, reproducible by anyone
  holding the tag. **No exclude list to maintain, so nothing left to
  rot** — this is the point, more than the leak it fixes
- The tag must be explicit, derived from the version or passed in, never
  inferred from `HEAD`
- **Refuse to build from an untagged or dirty state** rather than
  silently producing something unreproducible

Keep unchanged: the `noye-project-v<version>.tar.gz` filename (the `v` is
correct for artifacts), the flat layout from subject 03a, the version
read from `cargo metadata`, and the README companion.

## D-5 — answered: the archive carries `Cargo.lock`

Decided 2026-07-29, recorded as
[DEC-019](../../docs/src/decision-log.md#dec-019), superseding the second
half of DEC-006.

This makes the work *smaller*, not larger. `Cargo.lock` is tracked, so
`git archive` includes it by default:

- **No** `.gitattributes` with `export-ignore`
- **No** post-extraction step
- **Delete** `package.sh:32`'s `--exclude='Cargo.lock'` — it becomes dead
  once `tar` over the working tree is gone, but remove it explicitly so
  nobody reinstates the exclusion by pattern-matching the old script

Rationale, for context: a recipient who cannot reproduce the build cannot
verify anything the project claims about it. The accepted cost is that
recipients inherit pinned versions, so a dependency later found
vulnerable stays pinned for them until the next release — which argues
for release cadence, not for reversing this.

## Non-change scope

`package.sh`'s Japanese header comment (CON-09, subject 34), the archive
filename, the flat layout, the version source, and `ci.yml` — the release
workflow is a new file, not an extension of the CI one.

## Verify

| # | Test | Type |
|---|---|---|
| T-171 | The archive contains no path under `.git-exclude/`, `.claude/`, `target/` or `docs/book/` — the **untracked** set | **must fail first** |
| T-171a | The archive **does** contain deliberately-tracked configuration: `.cargo/config.toml`, `.vscode/settings.json` | guard |
| T-172 | Every path in the archive is tracked at the archived tag — compare against `git ls-tree -r --name-only <tag>` | **must fail first** |
| T-173 | Building twice from the same tag produces byte-identical archives | **must fail first** |
| T-174 | Building from a dirty or untagged tree is refused, not silently produced | **must fail first** |
| T-175 | The archive unpacks flat, and the filename carries the version | guard — subject 03a's property must not regress |
| T-176 | `Cargo.lock` **is present** in the archive (DEC-019) | guard |
| T-177 | Pushing a tag produces a GitHub Release with the archive attached — **confirmed on a real workflow run**, not inferred from the YAML | **must fail first** |

**T-172 is the one that cannot rot.** Comparing the archive to
`git ls-tree` at the tag needs no maintained list, so a future directory
cannot silently join the archive the way `.git-exclude/` did.

**T-177 is the one this subject exists for.** The whole point is moving
production from an unobserved local script to an observed run. Verifying
it by reading the workflow file would reproduce the defect in the act of
fixing it — see G-32 and G-33.

Use a scratch tag on a scratch branch, confirm the release and asset
appear, then delete both.

## Required documentation updates

- `docs/src/requirements.md` — PRQ-08 and PRQ-14 → `Implemented`;
  G-34 struck
- `docs/src/deployment.md` — the release procedure: push a tag, CI
  produces and attaches the artifact. The owner no longer runs anything
  locally
- `CHANGELOG.md`

## Done

- All seven tests pass; five baseline failures captured
- `rfcs/handoffs/evidence/subject-03d-tests.log` cites the release run
  directly and records the archive's entry count and top-level listing

## Escalate

| Situation | Do |
|---|---|
| A tag-triggered workflow needs permissions the repository does not grant | Report — release automation permissions are the owner's to set |
| Any already-published release is found carrying `.git-exclude/` | **Report immediately.** That is a disclosure question, not a packaging one. *(As of 2026-07-29 no release exists, so this is precautionary.)* |
