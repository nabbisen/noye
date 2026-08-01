# 06 — System-actor audit rows can be written

**Milestone** M1 · **Closes** G-03 · **Satisfies** FR-AUD-07, DR-INT-04, DR-INT-09
**Branch** `fix/06-audit-actor` · **Depends on** 05
**Governing artifact** — Gap **G-03** (§11)

## The defect

`audit_logs.actor_id` is `NOT NULL` with a foreign key to `users(id)`.
`log_system` writes the sentinel `"system"`, for which no user row
exists. The insert fails and the caller discards the result, so
system-originated audit events can be **silently absent** — and the chain
still verifies, because it covers only rows that were written.

## ⛔ Step 0 — reproduce before fixing

Confirm a `log_system` call against D1 actually fails. The premise is
that D1 enforces foreign keys.

**If it does not fail, stop and report.** G-03's stated consequence would
be wrong, only the design flaw would remain, and the priority changes.
Do not rewrite a hash-chained table to fix a defect nobody has
reproduced.

### ⚠️ The obvious local reproduction gives the wrong answer

**SQLite disables foreign keys by default, per connection.** Reproducing
Step 0 with the `sqlite3` CLI — the tool this project already uses for
the migration gate — succeeds, and would tell you G-03's premise is
false. Measured:

```
$ sqlite3 db "PRAGMA foreign_keys;"
0
$ sqlite3 db "INSERT INTO audit_logs (…, actor_id, …) VALUES (…,'system',…);"
                                                    ← succeeds, no error
$ sqlite3 db "PRAGMA foreign_keys=ON; INSERT … 'system' …;"
Error: FOREIGN KEY constraint failed
```

So a `sqlite3` reproduction proves **nothing** about D1 unless
`PRAGMA foreign_keys=ON` is set on the same connection, and even then it
only shows what SQLite does when asked, not what D1 does by default.

**Step 0 is a question about D1, and only a live D1 can answer it.** If
you cannot reach one, that is a stop-and-report in itself — say so rather
than substituting a local run. Whichever way the answer comes out, record
the `PRAGMA foreign_keys` value you observed alongside it; a reproduction
that does not state that value is not evidence.

## Build

Migration `sql/0004`, the standard SQLite table-rebuild:

1. Create `audit_logs_new` with identical columns, **without** the
   foreign key, plus `CHECK (actor_id != '')`.
2. Copy with an **explicit column list** — never `SELECT *`:

   ```sql
   INSERT INTO audit_logs_new
       (id, action_time, actor_id, actor_email, resource_type,
        resource_id, action_type, previous_value, new_value,
        result, ip_address, prev_hash, row_hash)
   SELECT id, action_time, actor_id, actor_email, resource_type,
          resource_id, action_type, previous_value, new_value,
          result, ip_address,
          NULL, NULL
     FROM audit_logs;
   ```

   Then, only where the source has them, carry the real hashes across —
   see the class note below.
3. Drop old, rename, recreate all four indexes: `idx_audit_time`,
   `idx_audit_actor`, `idx_audit_resource`, `idx_audit_row_hash`.

### ⚠️ Why the column list must be explicit

A **Class A** database — provisioned from tag 0.1.0 and never
re-migrated — has `audit_logs` **without** `prev_hash` / `row_hash`.
`SELECT *` supplies 11 values into a 13-column table and the migration
fails outright:

```
Parse error: table audit_logs_new has 13 columns but 11 values were supplied
```

Naming the eleven columns every class has, and defaulting the two hash
columns, makes this migration converge **all three classes** onto one
schema. Class A's rows arrive with NULL hashes, which `verify_chain`
already classifies as legacy rows and skips — the chain simply begins at
the next insert, exactly as designed.

For Classes B and C the hash columns exist and their values must be
preserved verbatim, or every `row_hash` breaks. Detect which case you are
in before copying; do not guess.

### This is the highest-risk change in the project

It rewrites the hash-chained table. Two properties make it safe.
**Verify both; assume neither:**

- The canonical serialization covers **column values only** — no rowid,
  no physical position, and not `prev_hash`/`row_hash` themselves. It is
  eleven fields plus a version tag (`hash.rs`, `AuditRowFields`), and
  `actor_id` is one of them. **Dropping the foreign key changes a
  constraint, not a value**, so every `row_hash` survives a faithful
  copy. Confirmed against `hash.rs` during pre-flight — but confirm it
  again rather than taking this line's word for it.
- **Order comes from the chain's links, not from any sort** (subject 05,
  DEC-020), so physical order after the rebuild is irrelevant.

  > **Corrected 2026-08-01.** This bullet previously read *"`verify_chain`
  > orders by `action_time ASC, id ASC`, not rowid."* **That is no longer
  > true** — subject 05 removed that ordering. The conclusion still holds,
  > and now holds for a stronger reason: nothing about the physical or
  > sorted order of rows enters the classification at all.

### What subject 05 bought this subject

T-25 asks that classification be *identical* before and after the
migration. **Before subject 05 that was not a well-defined test.** The old
verifier's result depended on how same-second rows happened to sort, so
"identical before and after" could differ between two runs over the same
unchanged data. Classification is now deterministic given the rows, which
is what makes T-25 a real guard rather than a coin toss. That is the whole
reason 05 came first.

### ⛔ Stop and report

Verify D1's behaviour on `PRAGMA foreign_keys` during the rebuild before
relying on it. **If D1 will not permit the procedure, stop and raise
it.** The sanctioned fallback is seeding a `system` sentinel user row —
smaller and chain-safe, but it puts a non-human principal in `/settings`
and only half-satisfies DR-INT-04. It needs a recorded decision, not a
quiet substitution.

### Do not

Do not change anything else about the table while you are in there. One
migration, one purpose.

## Verify

| # | Test | Type |
|---|---|---|
| T-24 | `log_system` inserts against a database with **zero** `users` rows | **must fail first** ¹ |
| T-25 | Chain classification identical immediately before and after the migration — **all four counts** (verified, legacy, tampered, orphaned), the same row identifiers in `tampered_rows` and `orphaned_rows`, and the same `cycle_at`. Assert the whole `ChainVerification`, not a subset | **guard — critical** |
| T-26 | All four indexes exist after the migration | guard |
| T-27 | An audit row with an empty `actor_id` is rejected | **must fail first** |
| T-28 | Deactivating or renaming a user alters no historical audit row | guard |
| T-29 | The retention pass's own `log_system` call now produces a row | **must fail first** |
| T-29a | The migration succeeds against a **Class A** database built from `git show 0.1.0:sql/0001_initial.sql`, leaving its rows with NULL hashes | **must fail first** |
| T-29b | Those NULL-hash rows are classified **legacy** — not tampered, and **not orphaned**. Both wrong answers are now reachable, and "legacy" is the only right one | **guard — critical** |
| T-29c | Against Classes B and C, every pre-existing `row_hash` is preserved byte-for-byte | **guard — critical** |

¹ Conditional on Step 0. If D1 does not enforce the foreign key, T-24
passes today and is a guard, not a must-fail-first.

## Done

- **All nine tests pass** (T-24 … T-29c); baseline failures captured
- `docs/src/requirements.md`: FR-AUD-07, DR-INT-04, DR-INT-09 →
  `Implemented`, G-03 struck
- `docs/src/architecture.md` and `security-posture.md` record that the
  actor is a snapshot, not a foreign key

## Escalate

Step 0 not reproducing · D1 blocking the rebuild · T-25 differing in any
respect → requirements architect, before proceeding.
