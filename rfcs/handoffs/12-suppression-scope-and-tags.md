# 12 — Suppression scope is exact and unambiguous

**Milestone** M2 · **Closes** G-08, G-09, G-27 · **Satisfies** FR-SUP-03, FR-TGT-10, DR-ENT-03, DR-INT-06
**Branch** same as subject 11 · **Depends on** 11 (same migration)
**Governing artifact** — Gaps **G-08**, **G-09**, **G-27** (§11)

## The defects

**G-09 — substring matching.** Tags live as a JSON array in
`targets.tags`, matched with `?3 LIKE '%' || target_tag || '%'`. A window
scoped to `api` also covers `api-v2` and `api-internal`; `prod` also
covers `production`. **Silent over-suppression** — the operator sees a
window scoped to one tag and has no way to discover it is silencing
three others.

**G-27 — wildcard leakage.** The stored tag lands on the *pattern* side
of `LIKE`, so a tag containing `%` or `_` becomes a wildcard. A window
scoped to `%` suppresses every target that has any tag at all. No
validation exists on tag content.

**G-08 — scope ambiguity.** Scope is a disjunction of target, tag and
global with no precedence and no exclusivity constraint, so a window
naming both a target and a tag applies more broadly than intended.

## Build

**Migration `sql/0006`, parts two and three.**

### Tag relation

```sql
CREATE TABLE target_tags (
    target_id TEXT NOT NULL REFERENCES targets(id) ON DELETE CASCADE,
    tag       TEXT NOT NULL,
    PRIMARY KEY (target_id, tag)
);
CREATE INDEX idx_target_tags_tag ON target_tags(tag);
```

Backfill from the existing JSON, then **drop `targets.tags`**.

> **Dropping the column touches four bind sites, two of them in a file
> M2a rewrote four days ago:**
>
> ```
> db/targets.rs:106   create  — input.tags
> db/targets.rs:163   update  — input.tags.or(current.tags)
> db/migration.rs:311 import  — tags = excluded.tags   (ON CONFLICT clause)
> db/migration.rs:336 import  — t.tags
> ```
>
> `db/migration.rs` now carries M2a's five `ON CONFLICT` upserts and
> seven `i64_to_d1` conversions. **Rewriting those statements can
> silently undo either** — an `INSERT OR REPLACE` returning reinstates
> G-22's cascade deletion; a raw `i64` bind reinstates G-38. Preserve
> both as you go.

`target_tags` becomes the single source of truth. `noye_shared::Target`
keeps `tags: Option<String>` as a *derived* value — read from the
relation, consumed into it on write. **The configuration document format
does not change**, which matters because subject 10 already changed it
once.

> Do not keep both the JSON column and the relation. Two sources of truth
> for one fact will drift, and the drift will be silent.

### Scope exclusivity

```sql
CHECK (NOT (target_id IS NOT NULL AND target_tag IS NOT NULL))
```

Resolve existing violating rows inside the migration and say in the
comment what you did with them.

> **Why a constraint rather than precedence logic.** FR-SUP-03 specified
> that target scope beats tag scope. Encoding that means two queries must
> agree forever. Making the ambiguous state unrepresentable is cheaper
> and cannot drift. The requirement has been restated to match.

### Exact matching, both queries

Replace the `LIKE` with:

```sql
EXISTS (SELECT 1 FROM target_tags tt
        WHERE tt.target_id = ?1 AND tt.tag = w.target_tag)
```

Exact by construction, metacharacter-proof, indexed.

**Also delete** `list_in_window`'s `tag_pattern = format!("%{}%", …)`.
It is bound as the LIKE *value*, where `%` is a literal character — it
has never done anything, and works only because the pattern side was
wildcarded. It will mislead the next reader.

## Verify

| # | Test | Type |
|---|---|---|
| T-58 | A window scoped to `api` does **not** apply to a target tagged `api-v2` | **must fail first** |
| T-59 | A window scoped to `prod` does **not** apply to `production` | **must fail first** |
| T-60 | A window scoped to `%` applies to **nothing** | **must fail first** |
| T-61 | A window scoped to `a_i` applies to nothing — `_` is a wildcard too | **must fail first** |
| T-62 | A window scoped to `api` **does** apply to a target tagged exactly `api` | guard |
| T-63 | A target with several tags matches a window scoped to any one of them | guard |
| T-64 | A window carrying both `target_id` and `target_tag` is rejected by the database | **must fail first** |
| T-65 | Every target tagged before migration `0006` is tagged after | guard |
| T-66 | Export → import round-trips tags unchanged | guard |
| T-66a | M2a's own regression scans still pass: no `INSERT OR REPLACE INTO` in `db/migration.rs`, five `ON CONFLICT` upserts, thresholds still routed through `i64_to_d1` | **guard — critical** |
| T-66b | In `scripts/check-d1-behaviour.sh`: a window scoped to tag `api` does **not** suppress a target tagged `api-v2`, and a window scoped to a tag containing `%` matches nothing but itself | **must fail first** |

T-61 is not redundant with T-60. `%` and `_` are different metacharacters
and a fix escaping one may miss the other. The relation-based match makes
both vacuous — assert it rather than assuming it.

## Done

- All nine tests pass; five baseline failures captured
- No `LIKE` remains in the module; `tag_pattern` is gone
- `docs/src/requirements.md`: FR-SUP-03, FR-TGT-10, DR-ENT-03, DR-INT-06
  → `Implemented`; G-08, G-09, G-27 struck

## Escalate

The tag backfill finding malformed JSON in `targets.tags` — report, do
not silently drop tags. Existing rows violating scope exclusivity —
report what you found before resolving.
