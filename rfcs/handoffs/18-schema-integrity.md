# 18 — Schema constraints, timestamps and indexes

**Milestone** M2 · **Closes** G-13, G-14, G-15 · **Satisfies** DR-INT-01, 02, 03, 07, 08
**Branch** `fix/18-schema-integrity` · **Depends on** subject 17 (same migration `0009`)
**Governing artifact** — Gaps **G-13**, **G-14**, **G-15** (§11)

## The defects

The guiding rule, from the requirements: **application-level validation
is necessary but not sufficient.** Data reaches these tables through the
API, the CLI, configuration import, and direct database access. A
constraint that exists only in Rust holds for one of those four.

**G-13** — no boolean, range or interval constraints anywhere.

**G-14** — schema defaults use `datetime('now')`, producing
`YYYY-MM-DD HH:MM:SS`; the application writes RFC 3339
`YYYY-MM-DDTHH:MM:SSZ`. Scheduling and window-overlap logic compare these
**as strings**, and `' '` (0x20) sorts below `'T'` (0x54). Mixed formats
compare *incorrectly but silently* — the worst failure mode available.

**G-15** — several live access paths lack indexes.

**These constraints are final.** DEC-008 settled that no tenant column is
coming, so nothing here will be reshaped later.

## Build — migration `sql/0009`

> **Renumbered 2026-08-11, before any migration was written.** This was
> `0007`. Subjects 15 and 16 also need migrations and had none reserved;
> since migrations apply in filename order and these subjects are worked
> 14 → 18, 15 takes `0007`, 16 takes `0008`, and 17/18 move to `0009`.
> `0005`–`0007` were reservations only — no file beyond `0004` exists —
> so this is a reservation change, not a renumbering of anything used.

### Constraints

- Every boolean column: `CHECK (col IN (0,1))` — **the list is below;
  do not derive it by searching**
- `port` 1–65535 · `expected_status` 100–599 · `timeout_sec` 1–300 ·
  `retry_count` 0–10 · `interval_minutes` 1–1440 · `tls_threshold_days >= 0`
- `success_threshold` and `failure_threshold` each `BETWEEN 1 AND 10`.
  **Zero must not be representable** — it would mean "transition on no
  evidence". **Both columns live on `targets`**, not `target_states` —
  moved by M2a, migration `0005`, DEC-012
- `maintenance_windows`: `CHECK (start_at < end_at)`, closing the schema
  half of FR-SUP-10

Tables needing a CHECK added require the table-rebuild procedure.

**The boolean columns — ten, across eight tables:**

| Table | Column(s) |
|---|---|
| `users` | `is_active` |
| `targets` | `is_disabled` |
| `check_results` | `is_success` |
| `maintenance_windows` | `suppress_notify`, `exclude_from_sla`, `is_active` |
| `notification_channels` | `is_enabled` |
| `target_notifications` | `on_down`, `on_up` |
| `retention_policies` | `archive_to_r2` |

> **⚠️ `target_states.consecutive_successes` and `consecutive_failures`
> are `INTEGER NOT NULL DEFAULT 0` and are NOT booleans.** Any sweep for
> "INTEGER columns defaulting to 0 or 1" catches them. `CHECK (col IN
> (0,1))` on a counter breaks the monitor on the third consecutive
> failure — with `failure_threshold` defaulting to 3, on precisely the
> transition this product exists to detect.

> **`on_down`/`on_up` are the only two booleans the application does not
> read through `bool_from_d1`** — `db/migration.rs:90-91` reads them as
> `i64` and compares `!= 0`. That is safe (a 0/1 `INTEGER` is well inside
> D1's Number range, so **G-41** does not reach them) and it is **not** a
> latent G-36. They still need the CHECK. Do not change the read path
> here; it is not this subject's.

> **`exclude_from_sla` did not exist when this subject was written** —
> M2b added it. It is in the list above.

### Timestamps

1. Replace every `DEFAULT (datetime('now'))` with
   `DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))`.
2. Normalise existing rows in the same migration.

> **There are 15 occurrences across 9 tables, and they are not all in
> `0001`.** `0001_initial.sql` has 11, `0004` has 1, and **`0006` has 3**
> — M2b's rebuilds of `targets` and `maintenance_windows` reproduced the
> old defaults verbatim four days ago, **including
> `targets.next_check_at`**, which is the scheduler's own column and
> exactly the `WHERE` comparison G-14 says fails silently. A sweep that
> reads only `0001` misses a third of the sites.
>
> **SQLite cannot alter a column default in place**, so this step rebuilds
> essentially every table in the schema. That is a much larger migration
> than these two bullets suggest, and it is why the preservation guard
> below matters more here than it would anywhere else.

> `stats.rs` carries a defensive parser accepting both forms. That helps
> read paths and does nothing for `WHERE` comparisons, which is where the
> damage is. Do not treat it as mitigation.

### Indexes

At minimum: `notification_channels(owner_id)`; the reverse
`target_notifications(channel_id)` lookup — the primary key is
`(target_id, channel_id)`, so channel→targets currently scans;
window-overlap lookups; `audit_logs(action_type)`.

Subject 12's `target_tags(tag)` index already closed part of this.

## Verify

| # | Test | Type |
|---|---|---|
| T-87 | Every boolean column rejects a value other than 0 or 1 | **must fail first** |
| T-88 | Each numeric range rejects one value below and one above its bound | **must fail first** |
| T-89 | Thresholds reject 0 and reject 11 | **must fail first** |
| T-90 | A window with `end_at <= start_at` is rejected by the database | **must fail first** |
| T-91 | A row written by schema default and one written by the application sort identically for the same instant | **must fail first** |
| T-92 | Valid values at **each boundary** are accepted — port 1 and 65535, timeout 1 and 300, retries 0 and 10 | guard |
| T-93 | Every listed access path is index-supported | guard |
| T-94 | **Every constraint and index present after `0008` is still present after `0009`** | guard |

**T-87 through T-94 all go in `scripts/check-migrations.sh`** — every one
is a schema refusal, an accepted boundary value, or an index's existence,
against a fresh `sqlite3` database with no D1 and no Wrangler. That script
already does these shapes: **T-25** (columns preserved across a
migration), **T-26** (four indexes exist after `0004`), **T-27** (a row
is rejected), **T-29a** (a migration refuses a fixture). And **G-37** is
still open, so `noye-core` has nowhere else to put them.

> **⚠️ T-94 is the one that protects three other subjects' work.** This
> migration rebuilds nearly every table, and **a SQLite table rebuild
> drops every index on the old table and carries only the columns the
> `INSERT ... SELECT` names.** Specifically it must preserve:
>
> - **`incidents`** — `idx_incident_one_open` (subject 15, `0007`) and
>   `opened_by`/`resolved_by` (subject 16, `0008`)
> - **`maintenance_windows`** — `CHECK (NOT (target_id IS NOT NULL AND
>   target_tag IS NOT NULL))` and the partial index `idx_maint_active`
>   (M2b, `0006`). **Dropping that CHECK silently reopens G-09's sibling
>   G-08**, in the milestone after it closed
> - **`targets`** — `0006`'s shape (no `tags` column; the `target_tags`
>   relation and its foreign key) and `0005`'s two threshold columns
>
> Write T-94 by enumerating `sqlite_master` after `0008` and after
> `0009` and diffing, not by listing what you remember adding.

**T-88 and T-92 are a pair.** A constraint written slightly wrong —
`BETWEEN 1 AND 65534`, an off-by-one — rejects valid data, and only the
accepting test catches it. Test both edges, in both directions.

**T-91 is the subtle one.** Build the fixture as two rows for the same
instant, one via schema default and one via the application, and assert
they order together. Do not assert on the string format — assert on the
comparison, because the comparison is what the scheduler depends on.

## Done

- All eight tests pass; five baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` — the wasm suites, not just `cargo check`

> **`0006` is not to be edited.** It is merged but unreleased, so
> DR-MIG-02 does not formally bind it — and the answer is still no.
> `0009` rebuilds. Nobody amends a migration that has already been
> applied to a database, including the ones on developer machines.
- `docs/src/requirements.md`: DR-INT-01, 02, 03, 07, 08 → `Implemented`;
  G-13, G-14, G-15 struck

## Escalate

A constraint rejecting rows already present in a live database → report
before anyone corrects the data. It means production data violates a rule
we are about to enforce, and that is worth understanding first.
