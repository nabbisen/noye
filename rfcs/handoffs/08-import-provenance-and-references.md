# 08 — Import provenance and owner references

**Milestone** M2 · **Closes** G-05, G-31 · **Satisfies** FR-TGT-08, FR-MIG-06, FR-MIG-08, FR-MIG-10
**Branch** `fix/08-import-references` · **Depends on** M1 merged
**Land together with subjects 09 and 10 — see the warning below.**
**Governing artifact** — Gaps **G-05**, **G-31** (§11)

## The defects

**G-05.** `crates/core/src/db/migration.rs:266` — `upsert_target` inserts
18 columns and omits `created_by` and `updated_by`, both `NOT NULL` with
no default (`sql/0001_initial.sql:37-38`). The insert cannot succeed.

**G-31.** `targets.owner_id` and `notification_channels.owner_id` are
`NOT NULL` foreign keys to `users(id)`, but `include_users` defaults to
**off** (external design §4.4, S-13). The default export therefore
carries no users, and importing it into a fresh deployment violates the
foreign key — with a raw constraint error, not a validation report.

**The default export cannot be imported into a fresh deployment**, which
is the primary stated use case for the configuration document.

## ⚠️ Do not ship this subject alone

G-05's `NOT NULL` failure currently *masks* G-22 (subject 09). Fixing
this without 09 converts a loud, safe failure into silent cascade
deletion of check results, incidents and channel attachments.

Subjects 08, 09 and 10 land in one branch.

## Build

**Provenance.** Add `created_by` and `updated_by` to
`noye_shared::Target`. On import set **both to the importing caller**,
not to the document's values.

> The document's values are user IDs from another deployment and mean
> nothing here. FR-MIG-08 requires equivalence with the normal path,
> where the creator is the caller. Origin is not lost — the envelope
> carries `source_deployment`, and the import writes an audit row
> (FR-MIG-09).

**Reference validation.** Before **any** write, resolve every referenced
`owner_id` against the target database and collect **all** unresolvable
references, then report them together. FR-MIG-06 requires all validation
errors in one pass. A dry run — the default, FR-MIG-05 — must surface
this without touching the database.

The message must tell the operator what to do:

> *"3 targets and 1 channel reference users that do not exist in this
> deployment. Re-export with 'Include users' enabled, or create the users
> first."*

### Do not

- Do not make `include_users` default to on. It is off deliberately — an
  export containing user email addresses is more sensitive than one
  without, and FR-MIG-04 makes it the operator's choice.
- Do not silently remap unresolvable owners to the importing caller.
  Reassigning ownership unasked is exactly the quiet behaviour the
  dry-run default exists to prevent.

## Verify

| # | Test | Type |
|---|---|---|
| T-37 | Import into an empty database with `include_users` on succeeds | **must fail first** |
| T-38 | An imported target carries the importing caller in `created_by` and `updated_by` | **must fail first** |
| T-39 | Import of a users-excluded document reports **every** unresolvable owner reference in one pass — assert the count | **must fail first** ¹ |
| T-40 | …and reports them in dry run, having written nothing | **must fail first** |
| T-41 | A document whose owners all resolve imports cleanly | guard |

¹ At baseline this fails on the `NOT NULL` constraint before reaching the
owner check. That still counts as failing-before-fix, but **record the
actual error** — it differs from the one the test detects, and the
distinction matters when re-running after the fix.

## Done

- All five tests pass; four baseline failures captured
- `docs/src/external-design.md` §8.2 records cross-reference validation
  and the provenance rule
- `docs/src/requirements.md`: FR-TGT-08, FR-MIG-10 → `Implemented`,
  G-05 and G-31 struck
