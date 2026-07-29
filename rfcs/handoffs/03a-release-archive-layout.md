# 03a — Release archive unpacks flat

**Milestone** M0 · **Closes** the archive half of G-24 · **Satisfies** PRQ-08
**Branch** `fix/03a-archive-layout` · **Depends on** subject 03
**Blocks** the first release tarball.
**Governing artifact** — Gap **G-24**, archive half (§11)

## Why this is here and not in subject 34

Scheduling defect, mine. Subject 34 fixes both halves of G-24 —
archive layout (PRQ-08) and Japanese comments (CON-09) — at **M5**.
But releases begin at **M0**, and the project's own rule is that
deliverables are tarballs.

So as planned, every release from v0.28.0 to v1.0.0 would ship an
archive that violates PRQ-08, with the fix arriving after the last one
that needed it. That is incoherent, and it surfaced the moment the first
tarball was due.

The archive-layout half is one line. It is pulled forward; subject 34
retains the language half, which blocks nothing.

Numbered `03a` rather than `37` because subjects are worked in numeric
order and this must precede `04`. Same suffix convention as `T-01a`:
inserted after the register was fixed, taking the preceding number.
Numbers are never reused or renumbered.

## The defect

`package.sh` applies:

```bash
--transform 's,^\.,noye,'
```

producing `archive.tar.gz → /noye/file1` — exactly the layout the
project rules mark ❌ Bad. The rule requires the archive to unpack
**flat**, directly into the extraction destination.

## Build

Remove the `--transform` line. Verify the resulting archive has no
intermediate parent directory.

### Do not

Do not touch `package.sh`'s Japanese header comment. That is CON-09,
subject 34, and bundling it here repeats the mistake this subject exists
to correct.

## Verify

| # | Test | Type |
|---|---|---|
| T-157 | The release archive unpacks flat, with no intermediate parent directory | **must fail first** |
| T-159 | The archive filename carries the version, single-sourced from the workspace manifest | guard |

Test numbers are taken from subject 34's register — they were always
this subject's tests; only their scheduling moved. Subject 34 retains
T-158 (non-English prose).

Verify by extraction, not by reading the script:

```bash
bash package.sh /tmp/pkgtest
tar tzf /tmp/pkgtest/noye-project-v0.28.0.tar.gz | head -5
```

Every path must begin at a repository top-level entry — `Cargo.toml`,
`crates/`, `docs/` — never `noye/`.

## Done

- Both tests pass; T-157's baseline failure captured
- `docs/src/requirements.md`: PRQ-08 → `Implemented`; G-24's archive
  half struck, its language half left open and still pointing at
  subject 34
- `CHANGELOG.md` updated

## Escalate

Anything that suggests a consumer depends on the nested layout →
requirements architect. PRQ-08 is unambiguous, but a downstream
expectation would be worth knowing before it breaks.
