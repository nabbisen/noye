# 19 — Identity keys on the OIDC subject claim

**Milestone** M2 · **Closes** G-16 · **Satisfies** FR-RBAC-07, FR-USR-05
**Branch** `fix/19-identity-sub` · **Depends on** subject 18
**Governing artifact** — Gap **G-16** (§11)

## The defect

The `users` table has **no `sub` column at all**.
`crates/gateway/src/lib.rs:675` passes `claims.sub` into session
creation, but user resolution is `SELECT * FROM users WHERE email = ?1`
(`db/users.rs:11`), and `email TEXT NOT NULL UNIQUE` is case-sensitive in
SQLite by default.

Two consequences: a provider returning different casing creates a
**duplicate account** for one person, and an email change at the identity
provider orphans the account — which is precisely what FR-RBAC-07 exists
to prevent.

FR-RBAC-07 was marked `Partial`; on the evidence it was `Not met`, since
there is no `sub` storage whatsoever.

## Build

1. Add `sub TEXT UNIQUE`.
2. Resolve identity by `sub`, falling back to email **once** to backfill
   an existing row, then storing the `sub` for subsequent logins.
3. `email TEXT NOT NULL UNIQUE COLLATE NOCASE`.

### The care this needs

The fallback path is an authentication path. Written loosely, an unknown
subject could match an existing row — a straightforward authentication
bypass. The fallback must match on email **and** only when the stored
`sub` is null, and must store the `sub` immediately.

## Verify

| # | Test | Type |
|---|---|---|
| T-94 | Two casings of one email address cannot become two accounts | **must fail first** |
| T-95 | An email change at the identity provider maps to the same account | **must fail first** |
| T-96 | An existing user logging in for the first time after migration is matched and backfilled, not duplicated | **must fail first** |
| T-97 | A subject with no user row is still refused with 403 | **guard — critical** |
| T-98 | A subject whose `sub` differs from a stored row's `sub` does not match that row | **guard — critical** |

**T-97 and T-98 guard the bypass.** They are the reason this subject is
separate from the rest of the schema work: it changes how identity
resolves, and the failure mode is not a wrong number but an
authentication hole.

## Done

- All five tests pass; three baseline failures captured
- `docs/src/requirements.md`: FR-RBAC-07, FR-USR-05 → `Implemented`, G-16 struck

## Escalate

T-97 or T-98 failing at any point → requirements architect, immediately.
