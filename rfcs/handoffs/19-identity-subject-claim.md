# 19 — Identity keys on the OIDC subject claim

**Milestone** M2 · **Closes** G-16 · **Satisfies** FR-RBAC-07, FR-USR-05
**Branch** `fix/19-identity-sub` · **Depends on** subject 18
**Governing artifact** — Gap **G-16** (§11)

## The defect

The `users` table has **no `sub` column at all**.
`crates/gateway/src/lib.rs:1027` passes `claims.sub` into session
creation, but user resolution is `SELECT * FROM users WHERE email = ?1`
(`db/users.rs:15`), and `email TEXT NOT NULL UNIQUE` is case-sensitive in
SQLite by default.

> **Line references corrected 2026-08-13** — both moved under M2a/M2b
> (`lib.rs:675` → `:1027`, `users.rs:11` → `:15`). The defect is
> unchanged.

Two consequences: a provider returning different casing creates a
**duplicate account** for one person, and an email change at the identity
provider orphans the account — which is precisely what FR-RBAC-07 exists
to prevent.

FR-RBAC-07 was marked `Partial`; on the evidence it was `Not met`, since
there is no `sub` storage whatsoever.

## Build — migration `sql/0010`

> **⚠️ This subject needs a migration; the handoff README said M2d had
> none.** `sub TEXT UNIQUE` can be added with `ALTER TABLE`, but
> **`COLLATE NOCASE` cannot be applied in place** — SQLite has no way to
> change a column's collation, so step 3 requires the full table-rebuild
> procedure. Migration number **`0010`**.

1. Add `sub TEXT UNIQUE`.
2. Resolve identity by `sub`, falling back to email **once** to backfill
   an existing row, then storing the `sub` for subsequent logins.
3. `email TEXT NOT NULL UNIQUE COLLATE NOCASE`.

> **`sub TEXT UNIQUE` permits multiple NULLs in SQLite** — verified
> directly, and the backfill in step 2 depends on it: every pre-existing
> row starts null and they must coexist. **Do not "tighten" it to `NOT
> NULL`**; that breaks every account created before this migration.

> **⚠️ `0010` rebuilds `users`, which `0009` rebuilt.** It must carry
> forward everything subject 18 put there:
> ```sql
> role        TEXT NOT NULL CHECK (role IN ('admin', 'member')),
> is_active   INTEGER NOT NULL DEFAULT 1 CHECK (is_active IN (0, 1)),
> created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
> updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
> ```
> Guarded by **T-98b**. Subject 18's own T-91 shipped unable to detect a
> reverted default, so write this guard against `sqlite_master`, not
> against a re-evaluated expression.

> **T-91 counts timestamp defaults exactly** — it asserts *ten*
> `DEFAULT (strftime(` clauses across the schema, which is what catches a
> rebuild silently dropping one. `0010` adds `sub TEXT UNIQUE` and no
> timestamp column, so the count stays at ten and T-91 needs no edit. **If
> that ever changes, move T-91's literal with it** rather than loosening
> it to `>=` — the exactness is the point, and T-26 had to be extended
> for the same reason one migration ago.

### Pre-existing case-duplicate emails: refuse, do not resolve

Adding `UNIQUE COLLATE NOCASE` **fails outright** if two rows differ only
by casing — which is the duplicate-account condition G-16 describes, so
expect it.

**Subject 15's precedent does not transfer.** That migration resolved
duplicate *incidents* automatically, because two incident rows for one
target are one event recorded twice. **Two user rows are two identity
records for one person**, and merging them means choosing which `id`
survives:

- **Six foreign keys reference `users(id)`** — three in `0001`, one in
  `0006`, two in `0009` — covering target and channel ownership. A merge
  silently reassigns owned resources.
- **`audit_logs` holds an actor *snapshot*** (`actor_id`, `actor_email`,
  subject 06), deliberately not a foreign key. History would keep
  pointing at the discarded `id` — unattributable, in the record whose
  entire purpose is attribution.

**Detect, name the conflicting addresses, and fail the migration.** Which
account survives is an operator decision, not a migration's.

### The care this needs

The fallback path is an authentication path. Written loosely, an unknown
subject could match an existing row — a straightforward authentication
bypass. The fallback must match on email **and** only when the stored
`sub` is null, and must store the `sub` immediately.

## Verify

| # | Test | Type | Instrument |
|---|---|---|---|
| T-98a | Two casings of one email address cannot become two accounts | **must fail first** | `check-d1-behaviour.sh` |
| T-95 | An email change at the identity provider maps to the same account | **must fail first** | `check-d1-behaviour.sh` |
| T-96 | An existing user logging in for the first time after migration is matched and backfilled, not duplicated | **must fail first** | `check-d1-behaviour.sh` |
| T-97 | A subject with no user row is still refused with 403 | **guard — critical** | `check-d1-behaviour.sh` |
| T-98 | A subject whose `sub` differs from a stored row's `sub` does not match that row | **guard — critical** | `check-d1-behaviour.sh` |
| T-98b | Every constraint and default on `users` after `0009` survives `0010` | guard | `check-migrations.sh` |

> **Why `T-98a` and not `T-94`.** `T-94` was assigned to subject 18 in
> the M2c pre-flight — the architect's mistake, made without checking
> forward — and it has since appeared in `scripts/check-migrations.sh`,
> in gate output, and in review request `051`. The README's rule is
> *renumber freely before evidence exists, never after*, so subject 18
> keeps it and this test takes a suffix on a number subject 19 owns.
> Shifting subject 19 to `T-95`–`T-99` would have cascaded through
> subjects 20 and 22–27.

**T-97 and T-98 guard the bypass.** They are the reason this subject is
separate from the rest of the schema work: it changes how identity
resolves, and the failure mode is not a wrong number but an
authentication hole.

**These run against the real login path**, not against a unit-tested
resolver. `noye-core` still cannot run wasm tests (**G-37**, open), and a
host test of identity resolution would be testing a mock of the thing
that matters.

### One risk to handle rather than discover

**Concurrent first logins.** The backfill reads `sub IS NULL` then writes
the `sub`; two simultaneous logins for one user can both read null, and
the second write fails the `UNIQUE` constraint. That **fails closed**,
which is the right direction — but it reaches a legitimate user as a
failed login. Re-resolve and continue rather than surfacing the error.
**Do not relax the constraint** — it is what makes the race safe.

## Done

- All six tests pass; three baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` — the wasm suites, not just `cargo check` (standing rule 8)
- `docs/src/requirements.md`: FR-RBAC-07, FR-USR-05 → `Implemented`, G-16 struck

## Escalate

T-97 or T-98 failing at any point → requirements architect, immediately.
