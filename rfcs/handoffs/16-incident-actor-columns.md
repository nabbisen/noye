# 16 — Incident actor columns carry one meaning each

**Milestone** M2 · **Closes** G-29 · **Satisfies** FR-INC-02, FR-SLA-06
**Branch** `fix/16-incident-actors` · **Depends on** subject 15
**Governing artifact** — Gap **G-29** (§11)

## The defect

`incidents.created_by` is set at open (`incidents.rs:10`) and overwritten
at resolve (`:26-27`, and `:44` with the literal `'system'`).

So the incident CSV's `created_by` column means "who opened it" for open
rows and "who resolved it" for resolved ones. An operator reconciling an
export cannot tell which.

One column carrying two meanings across a row's lifetime.

## Build

Split into `opened_by` and `resolved_by`.

> **`opened_by` is `'system'` for every row that exists.** `open()`
> (`db/incidents.rs:5`) takes no caller and writes the *literal*
> `'system'`; no route opens an incident manually. The defect is real —
> the resolve path clobbering the column is what makes the CSV
> ambiguous — but the fix is *preservation*, not attribution. **T-79
> should assert the opener survives resolution**, not that it names a
> person. Do not add a manual-open path to make the column interesting;
> that is a feature, and it is not in this subject.

**This changes external interface I-08.** The incident history export
goes from nine columns to ten. Per external design §14 that needs a
version note in `CHANGELOG.md` and a migration note for anyone parsing
the export.

> **This is the second breaking change to I-08 in the same unreleased
> version.** M2b renamed the *SLA* export's column 9
> (`maintenance_seconds` → `excluded_seconds`); this changes the
> *incident* export's column count. Write the changelog entry so the two
> read as one coherent breaking-change section naming each export
> separately — a consumer should not have to reconcile two entries to
> learn what happened to the exports. `scripts/check-changelog-section.sh`
> governs the section's shape.

### Do not

Do not silently redefine the existing column. A consumer parsing by
position or by name deserves to be told.

## Verify

| # | Test | Type |
|---|---|---|
| T-79 | An open incident records its opener and nothing else | **must fail first** |
| T-80 | Resolving records the resolver **without disturbing the opener** | **must fail first** |
| T-81 | Automatic resolution records `system` as resolver, opener untouched | **must fail first** |
| T-82 | The incident CSV has ten columns, with both names in the header | **must fail first** |

## Done

- All four tests pass; four baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` — the wasm suites, not just `cargo check`. **`Incident` gains two fields**, and a struct D1 deserializes into that gains a field is exactly what broke the `noye-shared` wasm suite on M2b's merge (fixture literals lacking the new column)
- `docs/src/external-design.md` §8.1 records the ten-column export
- `docs/src/requirements.md`: FR-INC-02 → `Implemented`, G-29 struck
