# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Note: changelog entries begin with the next release. Earlier history is
recorded in the project's release tags and the [docs/src/requirements.md](docs/src/requirements.md)
coverage table.

## [Unreleased]

### Added

### Changed

### Fixed

- **G-36**: every `bool`-typed field backed by a D1 `INTEGER` column
  (`User.is_active`, `Target.is_disabled`, `CheckResult.is_success`,
  `MaintenanceWindow.is_active`/`suppress_notify`,
  `NotificationChannel.is_enabled`, `RetentionPolicy.archive_to_r2`)
  failed to deserialize — D1 surfaces the column as a JS number, and
  `worker`'s internal `.unwrap()` on the deserialize result turned the
  mismatch into an uncaught Wasm trap rather than a returned error, so
  nothing was ever logged. **Not a regression** — the service has
  never worked against D1. Fixed with `bool_from_d1`, a
  `serde::de::Visitor` accepting a genuine boolean or the integer/float
  SQLite actually stores (non-zero is true; `NaN` is rejected rather
  than read as `true`), applied via `#[serde(deserialize_with = ...)]`
  to all seven fields. Confirmed against real local D1, not just unit
  tests — this project's first successful typed read against D1.

  **This fix made a second, more severe defect (G-38) reachable**:
  binding an `i64` to a D1 statement produces a JS `BigInt`, which
  D1's bind validation refuses. G-38 is not fixed by this change and
  is tracked separately (subject 07c) — see `docs/src/requirements.md`
  §11 for both.

### Removed

### Security

## [0.29.0] — 2026-08-02

### Added

- Migration `sql/0003_audit_retention_exemption.sql`, removing the
  seeded `audit_logs` retention policy row (idempotent). First
  migration after `0002`'s retirement (DEC-010) — the numbering gap
  is intentional.
- `scripts/changelog-section.sh <version>`, extracting the dated
  `CHANGELOG.md` section for a version. Exits non-zero when the
  section is missing or empty. Read-only — never writes this file.
- Migration `sql/0004_audit_actor_snapshot.sql`, rebuilding
  `audit_logs` without the `actor_id` foreign key so system-initiated
  events can be recorded. See Fixed, G-03, below.
- `db::audit::log_or_report`/`log_system_or_report`, and the
  `X-Audit-Warning` response header they lead to. See Fixed, G-26,
  below.

  **From this release onward, this changelog is what gets published.**
  `.github/workflows/release.yml` now sources the GitHub Release notes
  from this file's dated section for the tag, verbatim, instead of an
  auto-generated commit summary — and refuses to publish at all if the
  section is missing or empty. Writing a thin or missing entry here is
  no longer an internal omission; it is what ships.

### Changed

- `.github/workflows/release.yml` publishes with `gh release create
  --notes-file` (sourced from `scripts/changelog-section.sh`) instead
  of `--generate-notes`; the already-exists branch now also runs
  `gh release edit --notes-file`, so re-running the workflow against
  an existing release converges on the same notes rather than leaving
  whatever the first attempt published.
- `actions/checkout` (four uses in `ci.yml`, one in `release.yml`)
  bumped `v4` → `v7`; `actions/cache` (three uses in `ci.yml`) bumped
  `v4` → `v6` — both were targeting the Node 20 runtime, which GitHub
  now forces onto Node 24 with a deprecation warning on every run.
- `GET /api/admin/audit/verify` now returns a fourth classification,
  `orphaned_rows`, alongside `tampered_rows` — see G-30, below. The
  `/me/security` integrity-check card no longer reports "Chain intact"
  when tampering is absent but orphaned rows are present.

### Fixed

- **G-04**: `sql/0001_initial.sql` seeded a 365-day retention policy
  for `audit_logs`, and `crates/core/src/db/retention.rs` had a
  matching deletion arm — after 365 days the deletion broke the hash
  chain, the integrity check reporting the result as tampered. Fixed
  two ways: the seeded policy row is removed (see Added, above), and
  a new non-expiring data-class guard in `run_cleanup` refuses to
  delete from `audit_logs` regardless of any policy row present,
  checked before eligibility and consulting only the table name, never
  the policy row's other fields — a hand-reinserted policy row cannot
  change the outcome.

  **No existing deployment has lost an audit row to this defect.** The
  policy was 365 days and this project's oldest possible database is
  about three months old — no deployment has reached the retention
  cutoff, so no audit row has ever actually been deleted by it and no
  hash chain has been broken by it. This closes a latent defect, not a
  repair.

  **Operator action required:** run `wrangler d1 migrations apply` to
  apply `sql/0003_audit_retention_exemption.sql`. A database provisioned
  from `sql/0001_initial.sql` keeps the `audit_logs` policy row until
  `0003` runs; the code guard protects audit rows in the meantime, but
  the stale policy row should still be removed.

- **G-35**: `.github/workflows/release.yml` published release notes
  with `gh release create --generate-notes` — GitHub's automatic
  commit/PR summary — instead of the curated changelog entry a release
  is supposed to carry, and a tag with no dated changelog section
  still published successfully with a thin auto-generated body rather
  than failing. Neither this release's own migration step nor the fact
  that no audit row was lost (see G-04, above) would have reached an
  operator under the old mechanism. Fixed (see Added/Changed, above);
  confirmed on real, scratch-tagged Actions runs, including proving
  the release job fails — no release created — when the changelog
  section is missing.

- **G-30**: the audit chain's integrity check reconstructed row order
  by sorting on `action_time` and `id`, but neither column is
  monotonic with the order rows were actually written in — a routine
  same-second pair (a configuration import writes two) reported the
  trail as tampered roughly half the time; twenty rows in one second,
  essentially always. A tamper-evidence control that cries wolf is as
  damaging as one that stays silent. Fixed by reading order from the
  chain's own `prev_hash → row_hash` links instead of recovering it by
  sorting: the integrity check now walks the chain from genesis, and
  a deletion's unreachable successors are reported as a new, distinct
  `orphaned` class rather than misnamed as themselves `tampered`. The
  chain's writer derives the same head from the same walk, rather than
  a second query that can disagree with the reader — a fork no longer
  refuses the write, since an integrity control that anyone able to
  insert one row could turn into a kill switch is not one worth
  having. A cycle-termination defect found during review (a crafted
  row could hang the integrity check indefinitely) was fixed in the
  same round.

- **G-03**: `audit_logs.actor_id` was `NOT NULL` with a foreign key to
  `users(id)`, but system-initiated events (cron health checks,
  retention) write the sentinel actor `system`, for which no user row
  exists — the insert failed and the caller discarded the result, so
  an incident that opened and auto-resolved could leave no audit
  record at all, and the chain still verified because it covers only
  rows that were written. Fixed by the standard SQLite table-rebuild
  (`sql/0004`): the foreign key is replaced with
  `CHECK (actor_id != '')`, and the actor is now a snapshot captured
  at write time rather than a live reference, so a later deactivated
  or renamed user cannot alter what a historical row shows. Confirmed
  against real D1 before fixing anything: `PRAGMA foreign_keys`
  defaults to `1` and the insert is refused (the obvious local
  `sqlite3` reproduction gives the opposite answer, since bare
  `sqlite3` defaults that pragma off). Whether a database provisioned
  from tag 0.1.0 (predating the hash-chain columns) still exists
  anywhere was not checked — doing so would mean querying a real,
  credentialed database, which this project does not do (see
  `rfcs/handoffs/README.md` standing rule 7) — so `sql/0004` is scoped
  to databases that already carry `prev_hash`/`row_hash`, and is
  written to fail at prepare time, leaving the database untouched, if
  it ever meets one that doesn't (DEC-021).

  **Operator action required:** run `wrangler d1 migrations apply` to
  apply `sql/0004_audit_actor_snapshot.sql`.

- **G-26**: every `db::audit::log`/`log_system` call site —
  seventeen, across six `api/` files plus `monitor/engine.rs` — read
  `let _ = ... .await`, discarding the result unconditionally. A
  transient D1 failure on any of them produced a completed mutation
  with no audit row, and the hash chain still verified, since it
  covers only rows that exist. Fixed with three new helpers:
  `db::audit::log_or_report` — `#[must_use]`, returns a plain `bool` —
  for the fourteen sites with an HTTP response to warn on, and two
  "unattended" siblings that return nothing for sites with nothing
  further to do with an outcome: `log_system_or_report` (the two
  `monitor/engine.rs` sites, which run from the cron-driven monitor
  with no response at all) and `log_or_report_unattended`
  (`channels.rs`'s `send_test` error branch, which already returns an
  unrelated error and so has no successful response either). All three
  log at error level on failure (resource type, resource id, action
  type, actor), never the changed values, by construction: the pure
  formatter they share has no parameter to carry one through. A
  mutation whose audit write fails still returns 200 (the mutation
  happened; a 500 would say the opposite, and there is no transaction
  to roll back — DEC-011), now carrying `X-Audit-Warning: 1`, which the
  Gateway relays and every mutating page with a browser UI renders
  alongside its existing success message: *"Change applied. It could
  not be written to the audit log — please record it manually."*

### Removed

### Security

### Known issues

Twenty-three gaps remain open in `docs/src/requirements.md` §11's
conformance register, each named below with the subject that closes
it (none are scheduled before M2; this is the full list, not a
sample):

**M2 — configuration import** (subjects 08–10)
- G-05, G-31 (subject 08): the default export cannot be imported into
  a fresh deployment — `owner_id` columns are `NOT NULL` foreign keys
  and creator/updater columns don't round-trip.
- G-22 (subject 09): `INSERT OR REPLACE` on import fires
  `ON DELETE CASCADE`, silently destroying operational history while
  reporting success.
- G-06 (subject 10): import creates no per-target state row, so
  imported targets are not monitorable and thresholds don't
  round-trip.

**M2 — suppression and SLA** (subjects 11–13)
- G-07 (subject 11): a window marked non-suppressing still suppresses
  notifications; the SLA-exclusion query honours neither its own flag
  nor the active flag.
- G-08, G-09, G-27 (subject 12): suppression scope has no precedence
  rule between target/tag/global, tag matching is substring-based
  (over-suppresses), and a tag containing `%` or `_` acts as a `LIKE`
  wildcard.
- G-12 (subject 13): suppressed time is removed from measured
  downtime but not from the SLA denominator — reported SLA does not
  match its own on-screen definition.

**M2 — incidents** (subjects 14–17)
- G-10 (subject 14): automatically resolved incidents — the
  overwhelming majority — have no recorded duration and are missing
  from mean-time-to-recovery.
- G-11 (subject 15): at most one open incident per target is an
  application-flow property, not a database constraint.
- G-29 (subject 16): `incidents.created_by` means "opener" for open
  rows and "resolver" for resolved ones — one column, two meanings
  depending on state.
- G-17, G-28 (subject 17): the schema permits an `acknowledged`
  incident state and `degraded`/`maintenance` target-state values that
  nothing produces; the dashboard status breakdown still counts the
  latter two, so two of its categories are structurally always zero.

**M2 — schema hardening and identity** (subjects 18–20)
- G-13, G-14, G-15 (subject 18): boolean/range/interval constraints,
  a consistent timestamp format, and several access-path indexes are
  absent from the schema.
- G-16 (subject 19, closes **FR-RBAC-07**, `Not met`): identity
  resolves by email, case-sensitively — a provider's case variation
  can create a duplicate account for one person.
- G-19 (subject 20, closes **FR-AUTH-03**, `Not met`): no per-endpoint
  OIDC override exists; a provider that doesn't publish a discovery
  document is unsupported.

**M5 — process and documentation debt** (subjects 29, 33–35)
- G-18 (subject 29): notification delivery outcomes are logged to the
  console only; no delivery record is persisted, so an operator can't
  answer "was this incident notified?" after the fact.
- G-23 (subject 33): 40 files carry inline `#[cfg(test)] mod tests`
  where the project's own rule (PRQ-05) requires a sibling `tests.rs`.
- G-24, language half (subject 34): packaging comments and one
  `ROADMAP.md` phrase are Japanese against an English working-language
  requirement (CON-09) — the archive-layout half of G-24 is already
  closed.
- G-25 (subject 35): six `ROADMAP.md` → RFC links are dead, and two
  documents claim Slack receives generic JSON, which has been false
  since before v0.27.2.

**Not gap-tracked, and not remediable:** `DR-MIG-02` ("a released
migration's applied effect MUST NOT change") is `Not met` as a
standing historical fact — `sql/0001_initial.sql` shipped at tag
`0.1.0` and had DDL added to it at `0.27.2`, which is the direct cause
of the now-closed G-01. That already happened; no future subject
reverses it. Recorded here so a reader auditing migration history
knows why, not because a fix is pending.

### Rollback

**"Revert to `0.28.1`" means redeploying the old Workers code. It does
not mean the database goes back too** — there is no down-migration for
`sql/0003`/`sql/0004`, none is planned, and neither is reversed by
checking out an older tag. That distinction changes which of the four
defects this release closed actually come back:

**Reoccur immediately on a code-only rollback** (pure code fixes, no
migration involved):
- **Same-second writes can report false tampering** (G-30) —
  `verify_chain` reverts to recovering order by sorting
  `(action_time, id)`, which measured 0% clean across 2000 simulated
  runs of twenty rows written in one second.
- **Audit write failures are silently discarded everywhere** (G-26) —
  `X-Audit-Warning` and the operator-facing warning it feeds disappear;
  a mutation whose audit record failed to write goes unnoticed again.

**Do not reoccur on a code-only rollback** — the fix is in the
database, which stays as this release left it, and the old code's own
behaviour against the *new* schema does not reproduce the old defect:
- **The audit trail deletes itself after 365 days** (G-04) — `0.28.1`'s
  code still contains the deletion arm, but `sql/0003` already removed
  the seeded `audit_logs` policy row **permanently**; `run_cleanup`
  only ever acts on rows present in `retention_policies`, so there is
  nothing left to iterate for `audit_logs` regardless of which code
  version is running.
- **System-initiated audit events fail to record at all** (G-03) —
  `0.28.1`'s code issues the same insert either way; what changes is
  that `sql/0004` already dropped the foreign key on `actor_id`, so
  that insert now succeeds under old code too. The rebuilt table accepts
  everything the old one did, with one deliberate exception: an empty
  `actor_id`, which the replacement `CHECK` rejects and which no code
  path in any released version writes.

**Only a full database rollback** (restoring a backup from before this
release, not merely redeploying old code) reinstates all four —
including G-04 and G-03, since that restores the seeded policy row and
the foreign key along with the old code.

## [0.28.1] — 2026-07-30

### Added

- `.github/workflows/release.yml`: pushing a bare-version git tag now
  builds the release archive and attaches it, plus its README
  companion, to a GitHub Release for that tag. Production of the
  release archive moves from a local, human-run script to an observed
  workflow run (Subject 03d, G-34).

### Changed

- `package.sh` now builds the archive with `git archive` over the tag
  matching `[workspace.package].version`, instead of `tar` over the
  working directory with a manually maintained exclude list. It
  refuses to run against a dirty working tree, a version with no
  matching tag, or a `HEAD` not at the tagged commit (Subject 03d,
  G-34).
- The release archive now includes `Cargo.lock` (DEC-019, Subject
  03d) — the `--exclude='Cargo.lock'` this required is gone along
  with the `tar` invocation it belonged to.

### Fixed

- **G-34**: `package.sh` tarred the working directory rather than the
  tagged commit, excluding only `target/`, `Cargo.lock`, `dist/` and
  `.git/` — everything else on disk shipped, tracked or not. A
  v0.28.0 archive built by hand 2026-07-29 carried 300 entries
  including 54 paths under `.git-exclude/` (the review trail, review
  requests, roles documents, a 1.06 MB UI/UX PDF, the mockup bundle, a
  CI log archive) plus `.claude/settings.local.json` and `.vscode/`.
  It was also not reproducible: built from whatever was on disk, with
  nothing to indicate a dirty tree. Same shape as G-32 and G-33 — a
  mechanism nobody watched doing its job. Fixed via `git archive`
  (see Changed, above); confirmed on a real Actions run with a scratch
  tag (run `30506726912`): archived file list matched
  `git ls-tree -r --name-only <tag>` exactly, and two builds from the
  same tag were byte-identical.

### Removed

### Security

## [0.28.0] — 2026-07-28

M0: the release-blocking conformance gaps found by the v0.27.2
independent architecture review are closed. G-01 (a fresh database
could never finish provisioning), G-20 (retention silently deleted
unarchived records past 1000 per pass), and G-21 (the shipped
`wrangler.toml` published a working `GATEWAY_SHARED_TOKEN` and disabled
its own dev-fallback guard) are all fixed, tested, and independently
audited — see `rfcs/handoffs/` subjects 00 through 03a and
`.git-exclude/reviewed/011` through `016`. Every fix carries a
regression test that fails against the pre-fix commit (NFR-QA-09); the
evidence is in `.git-exclude/evidence/`.

Also folded in, all found during this release's own preparation rather
than scheduled for it: the toolchain pin (`rust-toolchain.toml`) and
the lint/format cleanup it revealed (44 clippy errors, widespread
`cargo fmt` drift — Subject 00); an unrelated, freshly-published
advisory (RUSTSEC-2026-0190, `anyhow`) fixed with a patch bump rather
than a suppression; the CI dependency-scan job having never actually
run (`cargo audit --locked` — cargo-audit takes no such flag — G-32);
and the release archive's nested `noye/` layout, pulled forward from
its originally-scheduled M5 fix to M0 (Subject 03a) because releases
begin at M0.

### Added

- Full software requirements specification installed at
  `docs/src/requirements.md`, replacing the former traceability matrix.
- External design specification at `docs/src/external-design.md`.
- Decision log at `docs/src/decision-log.md`, including DEC-008 (single
  tenant per deployment), DEC-009 (two roles), and DEC-010 (migration
  `0002` withdrawn).
- Requirements DR-LIF-07, DR-MIG-05, NFR-QA-10, NFR-SEC-14, NFR-SEC-15.
- Conformance gaps G-19 through G-30, from an independent review of
  v0.27.2 verified against source. G-30 records that the audit chain's
  writer and verifier disagree on row tie-breaking, so routine
  same-second writes can be reported as tampered.
- Requirements FR-AUD-11 and DR-INT-09.
- DEC-011: an audit write failure surfaces to the operator and does not
  fail the mutation.
- [RFC 0007](rfcs/proposed/007-atomic-audit-writes.md) — atomic audit
  writes, tracking the stronger "no change without a record" guarantee.
- `rfcs/handoffs/` — role-scoped work orders for implementer and tester, plus
  `.git-exclude/evidence/` for captured gate output.
- Requirements FR-MIG-10 (cross-reference validation before writing) and
  FR-MIG-11 (an import must not delete history belonging to objects it
  updates).
- [RFC 0008](rfcs/proposed/008-target-thresholds-on-target.md), accepted
  as DEC-012 — the consecutive-count thresholds move from
  `target_states` to `targets`, where the other decision criteria live.
  This is why they are lost in an export/import round trip.
- Gap G-31: `include_users` defaults off while `owner_id` is a `NOT NULL`
  foreign key to `users(id)`, so the default configuration export cannot
  be imported into a fresh deployment.
- `rust-toolchain.toml` pinning 1.91 with rustfmt, clippy and the
  `wasm32-unknown-unknown` target. The pin previously existed only in
  the CI workflow, so contributors on current stable saw 265 rustfmt
  diffs and clippy errors that CI would never report.
- Requirements FR-SUP-13 (a suppression window states silencing and
  SLA exclusion independently) and FR-SLA-09 (a fully-excluded window
  reports SLA as not applicable, never 100%).
- DEC-013: suppression windows gain a second, independent flag
  (`exclude_from_sla`) alongside `suppress_notify`, closing decision
  D-2. Scheduled for the Phase 3 work orders in `rfcs/handoffs/`.
- `scripts/check-migrations.sh` and a `migrations` CI job: every file in
  `sql/` is applied, in filename order, to a fresh SQLite database on
  every push and PR (DR-MIG-05, NFR-QA-10).
- A request-time schema assertion in `crates/core` (`db::audit::
  assert_hash_columns_present`) refuses to serve a request against a
  database that predates the audit hash-chain columns, naming the
  condition and the remedy, instead of letting every audit insert fail
  silently (gap G-26 is otherwise the only thing between this and months
  of invisible audit loss).

### Changed

- `crates/core/src/db/retention.rs` restructured: retention processing
  now selects, archives, and deletes eligible records in matched
  batches instead of archiving at most 1000 rows and then deleting
  every eligible row with no limit (DR-LIF-06, DR-LIF-07). A policy with
  `archive_to_r2 = 0` for `check_results` or `incidents` — classes that
  require archival before deletion — is now refused as a configuration
  error rather than honoured into an unarchived delete.
- DEC-017: retention batch size fixed at 100 as both the archive-select
  and delete-by-id chunk size, pending live verification against D1's
  actual bound-parameter limit (subject 36 closes it).
- `rfcs/handoffs/00-toolchain-hygiene.md`: retroactively records the
  toolchain-pin + lint/format cleanup delivered alongside Subjects 01–02
  as its own subject, and a standing rule that hygiene blocking a
  subject's verification gets its own PR, never bundled.
- `db::audit::assert_hash_columns_present` now caches a successful
  schema probe per isolate instead of querying `audit_logs` on every
  Core request. The condition is static for a deployment's lifetime;
  found by independent audit (`.git-exclude/reviewed/
  013-audit-subjects-01-02.md` F-2) as an uncached round-trip added to
  every request, including ones that never touch the audit log.
- Evidence-file naming moved from phase-era (`baseline-p0-p1.log`) to
  per-subject (`baseline-01.log`, `subject-01-tests.log`, …); documented
  in `.git-exclude/evidence/README.md`.
- **M0 complete (Subject 03).** `crates/gateway/wrangler.toml` and
  `crates/core/wrangler.toml` are no longer tracked; both are
  `.gitignore`d and replaced by `wrangler.toml.example` templates
  shipping `NOYE_ENV = "production"` and no secret values.
  `check_no_leaked_dev_fallbacks` in both crates no longer branches on
  `NOYE_ENV` — the denylist check runs unconditionally, in every
  environment, via a new pure `find_leaked_fallback` function
  (NFR-SEC-14, NFR-SEC-15). `crates/core/src/env_check.rs`'s
  `Environment` type is removed; Core had no other use for it.
- `docs/src/setup.md`, `docs/src/development.md` (new `.dev.vars` /
  token-generation step), `docs/src/dev-idp.md`, and
  `docs/src/security-posture.md` updated for the template + `.dev.vars`
  flow.

- Open decisions D-1 (multi-tenancy) and D-6 (role model) closed:
  single tenant per deployment, two roles. CON-08 moves to
  `Implemented`.
- DR-LIF-06 reworded: the requirement is that the deleted set equals the
  archived set, not merely that archival precedes deletion.
- Eight requirement statuses corrected against verified evidence:
  FR-AUTH-02, FR-AUTH-03, FR-RBAC-07, FR-NTF-12, DR-LIF-06, NFR-REL-03,
  NFR-SEC-09, PRQ-05, PRQ-08, CON-09.
- §11 remediation order re-sequenced against the delivery milestones.
- Open decision D-2 closed: suppression windows split "silence
  notifications" and "exclude from SLA" into two flags (DEC-013).
  FR-SUP-03 restated from a scope-precedence rule to a scope-exclusivity
  rule; FR-SUP-11 restated so its fourth semantic is conditional.

### Fixed

- **G-33**: the "Format, lint, check" CI job called `rustup toolchain
  install 1.91 --profile minimal --component rustfmt clippy` —
  `--component` takes a comma-separated list, and the space-separated
  form parses `clippy` as a second, invalid toolchain name. Traced to
  `5de978d`, the original 0.27.2 baseline commit: this job has never
  once completed, on any CI run in this project's history, before
  Format, Clippy, or Cargo check could run. NFR-QA-04/05/06 were marked
  `Implemented` on the strength of a job that had never executed.
  Corrected to `--component rustfmt,clippy`. Confirmed in a real
  GitHub Actions run (PR #2, run `30460161440`): all 5 jobs pass,
  including "Format, lint, check" for the first time in this
  project's history. Also confirmed the gate fails on a real
  violation via a discarded scratch branch (run `30460920132`).
- **G-32**: the CI dependency-scan job ran `cargo audit --locked`,
  which cargo-audit 0.22.2 rejects (`error: unexpected argument
  '--locked' found`) — the job exited before scanning anything. Not a
  documentation error: this is empirically why RUSTSEC-2026-0190 went
  undetected for a month of pushes and weekly crons until run by hand.
  Corrected to `cargo audit` in `.github/workflows/ci.yml`,
  `.git-exclude/evidence/README.md`, and
  `rfcs/handoffs/36-release-rehearsal.md`. Confirmed in a real GitHub
  Actions run (PR #2): 1173 advisories fetched, 224 crates scanned.
- **G-24 (archive layout half)**: `package.sh` applied
  `--transform 's,^\.,noye,'`, so every release archive from v0.1.0
  onward unpacked into a `noye/` parent directory — the layout the
  project's own rules mark as wrong. Removed; verified by extraction.
  Scheduled at M5 (Subject 34) despite releases beginning at M0, which
  would have shipped the forbidden layout in every release before the
  fix landed — pulled forward to Subject 03a. Subject 34 retains the
  Japanese-comment half (CON-09) unchanged.
- **G-21**: the shipped `crates/gateway/wrangler.toml` and
  `crates/core/wrangler.toml` set `NOYE_ENV = "development"` and a
  known-published `GATEWAY_SHARED_TOKEN` value in plain text, and the
  dev-fallback guard returned early whenever `NOYE_ENV == "development"`
  — the exact value the shipped file set. Deploying the repository
  unmodified yielded a permissive environment authenticated by a value
  published in the repository. The guard could not fire, by
  construction.
- `docs/book.toml` no longer sets `multilingual`, a field removed from
  the mdBook schema in 0.5. `mdbook build docs` previously exited 101,
  so the documentation tree did not render and the README's
  `mdbook serve docs` instruction did not work.
- All six `ROADMAP.md` links to RFCs pointed at `rfcs/NNNN-…` rather
  than `rfcs/proposed/NNN-…` and were dead.
- `ROADMAP.md` stated that Slack notifications use the same generic JSON
  as webhooks. A Block Kit adapter has shipped since before 0.27.2; the
  deferred work is enrichment, not introduction.
- `cargo fmt --all -- --check` failed on the pinned 1.91 toolchain
  across `crates/dev-idp/src/{handlers,jwt,keys}.rs` even after Subject
  00's cleanup claimed all three touched crates clean. Found by
  independent audit (F-1); `rfcs/handoffs/00-toolchain-hygiene.md`'s
  T-00a result corrected to record the failure before the fix, not just
  the fix.
- **G-20**: retention processing archived at most 1000 rows per table
  per pass but deleted every eligible row with no limit, permanently
  losing the excess with no archive copy and no error — the ordinary
  case for `check_results` at more than a few hundred targets. The
  string-interpolated `SELECT` in the old archival query and the bare
  `_ => continue` for an unrecognized `retention_policies` table are
  also fixed as part of the same change.
- **G-01**: a fresh database could never finish provisioning past
  `sql/0002_audit_hash_chain.sql`, which re-added columns `sql/
  0001_initial.sql` already carried — and, more severely, blocked every
  migration numbered after it. Root cause corrected 2026-07-28: `0001`
  had been amended in place after shipping under tag 0.1.0, which is
  itself a DR-MIG-02 violation; see DEC-010.
- Two comment typos claiming the hash-chain columns date to a "0.18.0"
  release that never existed (tags are `0.0.1`, `0.1.0`, `0.27.2`),
  across `sql/0001_initial.sql`, `crates/core/src/db/audit.rs`,
  `crates/core/src/db/audit/hash.rs`, `docs/src/deployment-
  observability.md`, and `docs/src/security-posture.md`.
- 44 pre-existing `clippy` lint errors across `crates/gateway`,
  `crates/core`, and `crates/dev-idp`, uncaught until this change
  because nothing had run `cargo clippy --workspace --all-targets
  --locked -- -D warnings` against the Rust 1.91 pinned toolchain since
  they were introduced. No behavioural change; all mechanical
  (`Range::contains`, `strip_suffix`, redundant casts, collapsible
  `if`s, and similar).
- Widespread `cargo fmt` drift left over from the same gap; reformatted.

### Removed

- `sql/0002_audit_hash_chain.sql`. The number `0002` is permanently
  retired (DEC-010); the next migration is `0003`.
- `crates/gateway/wrangler.toml` and `crates/core/wrangler.toml` are no
  longer tracked (G-21); see `wrangler.toml.example` in each crate.

### Security

- RUSTSEC-2026-0190: unsoundness in `anyhow::Error::downcast_mut()`
  (< 1.0.103), found while capturing this release's `cargo audit`
  evidence. Not caused by any change in this release; not previously
  documented. `anyhow` is used only by `noye` (cli) and `noye-dev-idp`
  — never a deployed Worker. Fixed with `cargo update -p anyhow`
  (1.0.102 → 1.0.104) rather than a suppression, since a patched
  release exists.

## [0.27.2] — 2026-05-04

A documentation-only release. No code changes; no behavioural change.
This release introduces the `rfcs/` directory carrying detailed
specifications for the priority items deferred in `ROADMAP.md`, so an
implementer picking up any of them does not have to reconstruct the
design choices.

### Added

- `rfcs/` directory at the workspace root with:
  - `rfcs/README.md` — index, workflow contract, and the rule for
    when an RFC graduates from `proposed` to `shipped`.
  - Six priority-item RFCs (`001`–`006`) covering: manual theme
    toggle, audit-log mirror via Cloudflare Logs, Turnstile activation
    on `/auth/login`, failed-login audit recording, high-contrast
    theme preset, and Slack-specific notification payload formatting.

### Changed

- `ROADMAP.md` — each entry that has a corresponding RFC now links to
  it so the high-level roadmap and the implementation-ready spec are
  reachable from each other.

## [0.27.1] — 2026-05-03

A documentation-and-tooling release. No code changes; no behavioural
change in production. The release records that the supply chain was
audited at this point and that one upstream advisory has been
explicitly evaluated and suppressed with reasoning.

### Added

- `.cargo/audit.toml` — `cargo-audit` ignore configuration carrying a
  documented suppression for `RUSTSEC-2023-0071` ("Marvin Attack" in
  the `rsa` crate). The entry includes the threat-model rationale
  inline so a future contributor reading `cargo audit` output can
  follow the chain back to the decision.

### Security

- Full RUSTSEC scan of the 0.27.0 lockfile (223 unique
  `(name, version)` pairs) recorded as a release artifact:
  - 0 confirmed CVE exposures
  - 0 unmaintained / informational notices
  - 1 documented suppression (`RUSTSEC-2023-0071`, see below)
- Documented `rsa` 0.9.10 (`RUSTSEC-2023-0071` / CVE-2023-49092)
  as a known-and-evaluated finding. The crate is reachable only via
  `noye-dev-idp`, a local-development OIDC stub that binds to
  `localhost` and is never deployed; the Marvin Attack threat model
  does not apply. See [`docs/src/security-posture.md`](docs/src/security-posture.md)
  for the full assessment, including the criteria for revisiting the
  decision.
