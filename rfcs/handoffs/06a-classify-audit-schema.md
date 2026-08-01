# 06a — Determine which database class exists before rebuilding the table

**Milestone** M1 · **Unblocks** 06 · **Informs** G-01's resolution note, G-03
**Branch** `fix/06a-classify-audit-schema` · **Depends on** nothing
**Work this before 06.** Numbered `06a` because it must run between 05
and 06; the next free number belongs to a later subject.

## Why this exists

Subject 06's migration `0004` cannot be one static SQL file if it must
serve **Class A** — a database provisioned from `sql/0001_initial.sql` as
it stood at tag `0.1.0`, with no `prev_hash`/`row_hash` columns. Proven in
`.git-exclude/review-request/020-…` and confirmed in
`.git-exclude/reviewed/029-subject-06-escalations.md`: SQLite resolves
column names at prepare time, so any file naming those columns fails to
prepare against a Class A source and — because a D1 migration file is
all-or-nothing — rolls back the table creation and index rebuild with it.

The owner was asked whether a Class A database exists and answered
**unsure**, choosing to make the answer *known* rather than assume it.
That is this subject.

| Class | From | Hash columns | `0004` can be static SQL? |
|---|---|---|---|
| **A** | `0.1.0`, never re-migrated | **no** | **No** |
| **B** | `0.1.0` then migrated (via `0002`) | yes | Yes |
| **C** | fresh on `0.27.2` (via `0001`) | yes | Yes |

**One Class A database anywhere changes subject 06 from an ordinary
migration into a schema-introspecting routine.** Nothing else about the
answer matters — B and C are handled identically.

## Build

### 1. `scripts/classify-audit-schema.sh <database-name>`

Wraps `wrangler d1 execute` and prints one of `CLASS_A`, `CLASS_BC`,
`NO_TABLE`, or `MALFORMED`, plus a human-readable line and the raw
evidence it decided from.

The discriminating query, **verified here against real fixtures** — a
Class A fixture built from `git show 0.1.0:sql/0001_initial.sql` and a
Class C fixture from the current `0001`:

```sql
SELECT
  (SELECT COUNT(*) FROM sqlite_master
    WHERE type='table' AND name='audit_logs')                    AS has_table,
  (SELECT COUNT(*) FROM pragma_table_info('audit_logs')
    WHERE name IN ('prev_hash','row_hash'))                      AS hash_cols;
```

| `has_table` | `hash_cols` | Verdict |
|---|---|---|
| 1 | 0 | **`CLASS_A`** — subject 06 is blocked on this database |
| 1 | 2 | **`CLASS_BC`** — static `0004` is fine |
| 0 | 0 | **`NO_TABLE`** — not a provisioned Noye database; not Class A |
| 1 | 1 | **`MALFORMED`** — exactly one hash column. Stop and report |

**The `has_table` half is load-bearing.** `pragma_table_info` on a
non-existent table returns zero rows, so the column count alone reports
`0` for an empty database and would misclassify it as Class A —
manufacturing the very finding this subject exists to test for. That is
the whole reason the query has two halves.

### 2. Distinguish B from C, for the record only

```sql
SELECT name FROM d1_migrations ORDER BY id;
```

`0002_audit_hash_chain.sql` present → Class B; absent → Class C. **This
does not change any decision** — record it, do not branch on it.

**Confirm the migrations table's real name** rather than assuming
`d1_migrations`. `crates/core/wrangler.toml.example` sets `migrations_dir`
but not `migrations_table`, so wrangler's default applies — and I have not
verified what that default is in wrangler v4 against a real database. If
the table is absent under that name, report it; do not guess a name.

### 3. Run it against every database the owner can reach

Local emulation and any remote D1. **Report the list of databases
checked, not only the verdicts** — "no Class A found" means nothing
without knowing what was looked at. If a database is known to exist but
cannot be reached, say so; an unreachable database is an unknown, not a
`CLASS_BC`.

### Do not

- **Do not modify any database.** This subject is read-only. No
  `wrangler d1 migrations apply`, no DDL, on anything.
- **Do not use `sqlite3` against a `.sqlite` file lifted out of D1** as
  the primary method. The point is to classify the databases that exist
  as D1 sees them.
- **Do not decide subject 06's shape.** Report the verdicts; the ruling
  is the architect's.

## Verify

| # | Test | Type |
|---|---|---|
| T-30a | The query returns `1\|0` against a Class A fixture built from `git show 0.1.0:sql/0001_initial.sql` | guard |
| T-30b | It returns `1\|2` against a fixture from the current `sql/0001_initial.sql` | guard |
| T-30c | It returns `0\|0` — **not** `CLASS_A` — against an empty database | **guard — critical** |
| T-30d | It returns `1\|1` → `MALFORMED` against a table carrying exactly one hash column | guard |
| T-30e | The script's verdict matches the raw query on every fixture — the classification logic itself does not drift from the evidence it prints | guard |

**T-30c is the one that matters.** A false Class A finding would re-scope
subject 06 into a structural change on the strength of an empty database.
The failure mode of this subject is not missing a Class A database; it is
inventing one.

Fold these into `scripts/check-migrations.sh` or a sibling gate so they
re-run — the same reasoning as subject 04a §4. A classifier verified once
against fixtures that no longer exist is not verified.

## Done

- The five tests pass, wired into CI
- **A report to the architect**: every database checked, its verdict, and
  its raw `has_table|hash_cols` pair. That report is the deliverable —
  the script is how it is produced

## Escalate

- **Any `CLASS_A` verdict** → architect immediately. Subject 06 is
  re-scoped and this is the finding that does it.
- **Any `MALFORMED` verdict** → architect. A table with one hash column is
  not a class this project has ever described.
- **The migrations table is not where step 2 expects** → report the real
  name; do not search for it by trial.
