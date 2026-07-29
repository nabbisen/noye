# 34 — Documentation language

**Milestone** M5 · **Closes** the language half of G-24 · **Satisfies** CON-09
**Branch** `chore/34-language` · **Depends on** nothing — may run in parallel
**Governing artifact** — Gap **G-24**, language half (§11)

> **The archive-layout half moved to [subject 03a](03a-release-archive-layout.md)**
> and was worked at M0. It blocked the first release tarball, so leaving
> it here would have meant every release from v0.28.0 onward shipping a
> known PRQ-08 violation with the fix arriving after the last one that
> needed it. This subject retains the language half, which blocks nothing.

## The defect

**CON-09.** `package.sh`'s entire header comment block is Japanese.
CON-09 requires English for all documentation and code comments. The
`ROADMAP.md` instance was corrected on 2026-07-28; this one remains.

## Build

1. Translate `package.sh`'s header comment block to English.
2. Confirm no other source or documentation file carries non-English
   prose.

## Verify

| # | Test | Type |
|---|---|---|
| T-158 | No source or documentation file contains non-English prose | **must fail first** |

T-157 and T-159 moved to subject 03a with the archive-layout work.

## Done

- T-158 passes
- `docs/src/requirements.md`: CON-09 → `Implemented`; G-24 fully struck
  once this lands (its archive half closed at M0 by subject 03a)
