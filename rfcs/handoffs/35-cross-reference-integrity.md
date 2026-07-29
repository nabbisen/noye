# 35 — Cross-references resolve

**Milestone** M5 · **Closes** G-25 · **Branch** `chore/35-cross-refs`
**Depends on** subjects 30–32 (RFCs must have moved to `done/` first)
**Governing artifact** — Gap **G-25** (§11)

## The defect

The six dead `ROADMAP.md` → RFC links and the false Slack claim were
corrected on 2026-07-28. This subject is the periodic check that keeps
them honest, run once every RFC of this milestone has moved folders.

The RFC lifecycle policy names rotten cross-references as an anti-pattern
and "status fields that lie" as another. Both were live at v0.27.2.

## Build

1. `grep -rn 'rfcs/[0-9]' . --include='*.md'` — confirm every RFC
   reference resolves, including from `docs/src/`.
2. Confirm every RFC that shipped during subjects 10, 17, 22, 28, 30, 31
   and 32 moved to `done/` or `archive/` with its `Status` updated **in
   the same change**, per the workflow in `rfcs/README.md`.
3. Confirm `rfcs/README.md`'s index and order table match the folders.
4. Confirm `docs/src/requirements.md` §11 has every closed gap **struck,
   not deleted** (§15).

## Verify

| # | Test | Type |
|---|---|---|
| T-160 | Every `rfcs/` reference in every Markdown file resolves | **must fail first** |
| T-161 | Every RFC in `done/` reads `Status: Implemented (x.y.z)`; every RFC in `proposed/` reads `proposed`; every RFC in `archive/` reads `Withdrawn` or `Superseded` | **must fail first** |
| T-162 | `rfcs/README.md`'s index matches the folder contents | guard |
| T-163 | Every gap closed in subjects 01–34 is struck, not deleted | guard |

Make T-160 and T-161 mechanical and part of the gate. They are cheap, and
both were violated at v0.27.2.

## Done

- All four tests pass
- `docs/src/requirements.md`: G-25 struck
