# 01 — Migration set applies to an empty database

**Milestone** M0 · **Closes** G-01 · **Satisfies** DR-MIG-01, DR-MIG-05, NFR-QA-10
**Branch** `fix/01-migration-apply` · **Depends on** nothing
**Governing artifact** — Gap **G-01** (`docs/src/requirements.md` §11)

## The defect

`sql/0001_initial.sql:148-149,155` already declares `prev_hash`,
`row_hash` and `idx_audit_row_hash`. `sql/0002_audit_hash_chain.sql`
adds the same two columns with unconditional `ALTER TABLE`.

`wrangler d1 migrations apply` stops at the first failure, so **`0002`
blocks every subsequent migration.** Subjects 04, 06, 10, 11, 12, 17 and
18 add migrations `0003`–`0007`; none can apply while `0002` sits there.

*Measured, not assumed:* a fresh database provisions fine — `0001` alone
creates all ten tables **with** the hash columns and is fully writable.
The gap register previously said "a fresh deployment cannot be
provisioned"; that overstated one thing and understated the pipeline
blockage, and was corrected on 2026-07-28.

## Build

1. **Delete `sql/0002_audit_hash_chain.sql`.** Do not modify `0001`
   except for step 2.
2. Fold `0002`'s header prose into the comment block at `0001:146` —
   what `prev_hash` / `row_hash` mean, and the legacy-row rule — so the
   reasoning survives the file. **Do not "fix" `0,18.0` to `0.18.0`** at
   `0001:146` or `crates/core/src/db/audit.rs:11` — 0.18.0 never existed
   (tags are `0.0.1`, `0.1.0`, `0.27.2`). Replace it with `0.27.2`, the
   release both the amendment and `0002` actually landed in.
3. Add a build gate: apply every `sql/*.sql` in filename order to a fresh
   SQLite database, fail on error. Wire into `.github/workflows/ci.yml`
   on push and pull request.
4. **Add a schema assertion on the Core request path**, alongside the
   existing `env_check` fail-closed pattern: verify `audit_logs` carries
   `prev_hash` and `row_hash`, and refuse with a named, actionable error
   if not.

   Without this, a **Class A** database deployed on v0.28.0 reports
   migrations complete, then fails every audit insert with *no such
   column* — which subject 07 has not yet surfaced, so the failures are
   silently discarded (G-26). The assertion turns a silent, months-long
   evidence gap into one legible error at the first request.

   The message must name the condition and the remedy, e.g.
   *"audit_logs is missing prev_hash/row_hash — this database predates
   0.27.2 and has not been reconciled. Apply migration 0004."*

   **Probe once per isolate, not once per request.** The condition is
   static for a deployment's lifetime — the columns cannot appear or
   disappear between two requests to the same isolate, since that
   requires a migration and therefore a deploy. An uncached probe adds a
   D1 query to every Gateway→Core call in latency and billed reads,
   including calls that never touch `audit_logs`.

### Do not

- **Do not remove the columns from `0001` instead.** This looks like the
  same fix and is not. Three database classes exist, split by the 0.1.0
  and 0.27.2 releases:

  | Class | From | Hash columns | State |
  |---|---|---|---|
  | **A** | 0.1.0, never re-migrated | **no** | `0001` (old) applied |
  | **B** | 0.1.0 then migrated | yes, via `0002` | both applied |
  | **C** | fresh on 0.27.2 | yes, via `0001` | `0001` applied, `0002` failing |

  Removing the columns from `0001` leaves Class C still failing at
  `0002`, because `0001` is already recorded as applied there. Deleting
  `0002` clears Classes B and C. **Class A is handled by subject 06's
  rebuild**, which names its columns explicitly for exactly this reason.
  Recorded as DEC-010, premise corrected 2026-07-28.
- Do not renumber. `0002` is retired; the next migration is `0003`. The
  gap is intentional.
- Do not use `ADD COLUMN IF NOT EXISTS` — unsupported, and DR-MIG-03
  forbids relying on it.

### Note

The gate does not need D1. A fresh SQLite database is a faithful
substrate for "does this DDL apply in order", and keeps the gate fast and
credential-free. `verify_chain`'s legacy-row handling is unaffected; no
Rust change is needed.

## Verify

| # | Test | Type |
|---|---|---|
| T-01 | Applying every `sql/*.sql` in filename order to an empty database exits 0 | **must fail first** |
| T-02 | `audit_logs` has `prev_hash`, `row_hash`, `idx_audit_row_hash` after application | guard |
| T-03 | A deliberately broken migration fails the build gate | guard |
| T-01a | A **Class A** database — built from `git show 0.1.0:sql/0001_initial.sql` — is **reported** as lacking the hash columns, rather than silently treated as healthy | **must fail first** |

**T-01 is the G-01 reproduction.** Capture its failure against the
pre-fix commit before any fix lands — that evidence is unobtainable
afterwards.

**T-01a exists because deleting `0002` is safe only for Classes B and C.**
It does not fix Class A; it makes Class A *visible*, so nobody assumes
migrations completing means the schema is right. Class A is repaired by
subject 06's rebuild.

*Numbered `T-01a`, not `T-04`: it was added after the register was fixed,
and `T-04` already belongs to subject 02. See the numbering rule in
`rfcs/handoffs/README.md`.*

## Done

- All three tests pass; T-01's baseline failure captured in `rfcs/handoffs/evidence/`
- No file in `sql/` begins `0002`
- Gate runs on push and PR
- Docs: `docs/src/setup.md:60` (drop `0002` from the example ordering,
  note the retired number), `:68` (delete the paragraph on applying
  `0002` to a pre-0.18.0 deployment — it describes something that never
  worked), `docs/src/deployment.md:108,123` (reword the generic
  `0002_add_field.sql` example)
- `docs/src/requirements.md`: DR-MIG-01 → `Implemented`, G-01 struck

## Escalate

A database class not in the table above → requirements architect. The
three-class model was itself a correction; treat it as checkable, not
settled.
