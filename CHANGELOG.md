# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Note: changelog entries begin with the next release. Earlier history is
recorded in the project's release tags and the [docs/src/requirements.md](docs/src/requirements.md)
coverage table.

## [Unreleased]

### Added

- `scripts/check-d1-behaviour.sh` gains a fourth assertion: a scheduled
  tick makes exactly one of two things observable — a retention pass
  ran, or the skip was logged, naming the minute — never neither. See
  Fixed, G-43, below.
- `scripts/check-d1-behaviour.sh` gains three more assertions against
  real local D1: (e) a `suppress_notify=0` window does not suppress a
  notification and an `exclude_from_sla=0` window does not move the SLA
  figure (T-52b); (f) tag scoping is exact — a window scoped to `api`
  does not suppress a target tagged `api-v2`, and one scoped to a tag
  containing `%` matches nothing but a target tagged exactly `%`
  (T-66b); (g) a window covering an entire report period excludes the
  whole denominator and reports SLA as not applicable, not a claimed
  100% (T-70a). See Fixed, G-07/G-08/G-09/G-12/G-27, below.
- `/maintenance` gains a real suppression-window create form: three
  named situations (DEC-013) and target/tag/all-targets scope, both as
  radio groups, submitted through a native `<form method="post">` —
  works with scripting disabled (NFR-A11Y-10). Previously the only way
  to schedule a window was `POST /api/maintenance` by hand. The listing
  table gains an "SLA" column stating `excluded`/`counted` in text, next
  to the existing `suppressed`/`unaffected` notifications column
  (NFR-A11Y-03).
- `scripts/check-d1-behaviour.sh` gains an eighth assertion (h): a
  target driven down then up through two real scheduled ticks ends with
  a non-null `duration_sec` on its incident and a non-null MTTR. See
  Fixed, G-10, below.
- `scripts/check-migrations.sh` gains three assertions (T-76–T-78): a
  second open incident for the same target is refused by the database
  after migration `0007`, resolving the first allows a new one, and
  pre-existing duplicates are resolved by the migration itself. See
  Fixed, G-11, below.

### Changed

- **BREAKING (I-08): both CSV exports change shape in this unreleased
  version.**
  - **SLA export** (`/api/stats/sla.csv`): column 9 is renamed
    `maintenance_seconds` → `excluded_seconds`. Under DEC-013's
    suppression/SLA split these are no longer the same fact — a window
    can silence notifications without excluding SLA time, or the
    reverse — so the old name no longer describes what the column
    reports. See Fixed, G-12, below.
  - **Incident history export** (`/api/stats/incidents/:id.csv`): goes
    from nine columns to ten. Column 9, `created_by`, is replaced by
    `opened_by`; a new column 10, `resolved_by`, is added (empty for
    open incidents). See Fixed, G-29, below.

  Anyone parsing either export by column name or position needs to
  update. Both changes are unreleased and land in the same version, so
  they are called out together here rather than as two entries a reader
  has to reconcile.
- `crates/core/src/monitor/engine.rs`'s retention gate now decides from
  the scheduled invocation's own nominal time, not the wall clock; the
  decision is a pure function, host-tested. See Fixed, G-43, below.
- `maintenance_windows` gains a second, independent flag,
  `exclude_from_sla` (migration `0006`), alongside the existing
  `suppress_notify` — DEC-013's three named situations (planned
  maintenance, known external outage, expected noise). See Fixed, G-07,
  below.
- `targets.tags` (a JSON-array TEXT column) is replaced by a normalized
  `target_tags` relation (migration `0006`, backfilled before the
  column drops). `Target.tags` in the JSON API and configuration
  document is unchanged — it's now a derived field, read from and
  written through the relation. See Fixed, G-09/G-27, below.
- `stats::compute_sla`'s SLA-uptime denominator now excludes suppressed
  time (`effective_window = window_seconds − excluded_seconds`), not
  only the numerator. See Fixed, G-12, below.
- `incidents.created_by` (one column, two meanings across a row's
  lifetime) is replaced by `opened_by` and `resolved_by` (migration
  `0008`); `created_by` is left in place, unused, until a later subject
  rebuilds the table for unrelated reasons. See Fixed, G-29, below.
- `incidents` gains a partial unique index, `idx_incident_one_open`
  (migration `0007`), enforcing at most one open incident per target at
  the database level. See Fixed, G-11, below.

### Fixed

- **G-43: retention's hourly trigger read the wall clock, not the
  scheduled event — so a late or retried invocation could silently
  skip an hour of retention, with nothing logged either way.**
  `crates/core/src/monitor/engine.rs` gated `run_cleanup` behind
  `chrono::Utc::now().format("%M") == "00"`, where `now` is read at
  handler start; the cron fires every minute, and the invocation's own
  scheduled time was received and discarded. Cloudflare's cron
  triggers are best-effort, not exact — an invocation nominally for
  00:00 that actually starts at 00:01 read wall-clock "01", the
  condition was false, and the branch simply did not fire. The same
  project shape as G-21, G-32 and G-33: a control that appears
  configured, may never execute, and says nothing when it doesn't.
  Fixed by deciding from the invocation's own `event.schedule()`
  instead, extracted into a pure `retention_trigger` function so the
  decision is an ordinary host unit test rather than something needing
  a live Worker runtime — the same pattern `decide_transition` and
  `compute_cutoff` already use. Both non-firing paths are now logged:
  a normal skip names the minute; an unparseable schedule is a
  distinct error.

  **DR-LIF-06 (a retention pass deletes only what it archived) is
  still not assertable on demand in CI.** `wrangler dev --local`'s
  scheduled-event simulation does not propagate a controllable nominal
  time through `event.schedule()` — confirmed against real `workerd`
  that it always returns the wall clock regardless of what's
  requested, traced to workerd's own local-mode simulation rather than
  this project's code or the `worker` crate. Recorded honestly rather
  than asserted anyway: the CI gate now asserts the *silence* half of
  this fix (retention ran, or the skip was logged — never neither),
  and the timing claim itself moves to `scripts/deployment-verify/`
  for confirmation against real Cloudflare infrastructure during the
  next deployment session.

- **G-07: a suppression window's flags did not control what they said
  they controlled.** `is_under_maintenance` (notifications) checked only
  `is_active`, so a window explicitly marked `suppress_notify = false`
  still silenced alerts; `list_in_window` (SLA) checked neither flag at
  all, so a deactivated window still moved the SLA figure. Fixed by
  filtering `is_active = 1 AND suppress_notify = 1` and
  `is_active = 1 AND exclude_from_sla = 1` respectively, and by adding
  the second flag those filters needed (`exclude_from_sla`, migration
  `0006`). Confirmed against real local D1
  (`scripts/check-d1-behaviour.sh`, T-52b): a `suppress_notify=0` window
  no longer suppresses a real scheduled-tick notification, and an
  `exclude_from_sla=0` window no longer moves `excluded_seconds`, while
  the same shape of window with the flag set does.

- **G-09 / G-27: tag scoping matched by substring, and the stored tag
  sat on the pattern side of `LIKE`.** A window scoped to `api` also
  matched `api-v2` (G-09); a window scoped to a tag containing `%` or
  `_` matched every tagged target, since those characters are `LIKE`
  wildcards on the pattern side (G-27). Fixed by replacing
  `targets.tags` (a JSON-array TEXT column matched with
  `LIKE '%' || target_tag || '%'`) with a normalized `target_tags`
  relation and an exact `EXISTS` join — metacharacter-proof by
  construction, not merely escaped. Confirmed against real local D1
  (T-66b): a window scoped to tag `api` does not suppress a target
  tagged `api-v2`, and one scoped to `%` matches nothing but a target
  tagged exactly `%`.

- **G-08: a window could name both a target and a tag, applying more
  broadly than either scope alone.** Fixed with
  `CHECK (NOT (target_id IS NOT NULL AND target_tag IS NOT NULL))` on
  `maintenance_windows` (migration `0006`) — the ambiguous state is
  unrepresentable rather than resolved by precedence logic two queries
  would otherwise have to keep agreeing on. Pre-existing violating rows
  are resolved before the constraint is added, target scope winning
  (FR-SUP-03).

- **G-12: the SLA-uptime denominator did not shrink for excluded time,
  only the numerator did.** `stats::compute_sla` subtracted excluded
  downtime from the *numerator* while leaving the denominator at the
  full window length — "ignore outages during maintenance", not the
  specified "maintenance time did not happen for SLA purposes". Fixed
  by computing `effective_window = window_seconds − excluded_seconds`
  and reporting the ratio against that; when the whole window is
  excluded, SLA is now `None` ("not applicable", rendered as an em dash
  — the same convention `mttr_seconds` already used) rather than a
  claimed 100%. Confirmed by five new unit tests and, end to end
  against real local D1 (T-70a), a window covering an entire 24h report
  period reporting `excluded_seconds` equal to the full window and
  `sla_uptime_ratio: null`.

- **G-10: automatically-resolved incidents — the overwhelming
  majority — recorded no duration, so mean-time-to-recovery was
  computed over an unrepresentative minority and presented as if it
  were the whole picture.** `db::incidents::resolve` (manual
  resolution) computed and stored `duration_sec`; `auto_resolve` did
  not. Fixed by computing `duration_sec` in SQL inside `auto_resolve`'s
  own `UPDATE` (`opened_at` to the resolution instant, per row — the
  statement resolves every open incident for the target at once, so the
  computation can't be a single value computed in Rust), not as a raw
  `i64` bind (G-38's shape). `stats::compute_sla`'s MTTR also derives
  duration from `resolved_at − opened_at` when `duration_sec` is null,
  so incidents auto-resolved before this fix are not permanently
  excluded from reporting. Confirmed against real local D1
  (`scripts/check-d1-behaviour.sh`, T-75a): a target driven down then up
  through two real scheduled ticks ends with a non-null `duration_sec`
  and a non-null MTTR.

- **G-11: at most one open incident per target was a property of
  application flow only, with nothing to stop re-entrant scheduling,
  manual operations, or future concurrency from producing duplicates.**
  Fixed with a partial unique index,
  `CREATE UNIQUE INDEX idx_incident_one_open ON incidents(target_id)
  WHERE status = 'open'` (migration `0007`). Pre-existing duplicates are
  resolved by the migration itself before the index is created: the
  earliest-opened incident per target stays open, later duplicates are
  force-resolved with a `resolution_note` recording why. Confirmed via
  `scripts/check-migrations.sh` (T-76, must-fail-first: a second open
  insert succeeds before `0007`, is refused after; T-77: resolving the
  first allows a new one; T-78: the migration's own duplicate resolution
  ran correctly) — the database, not the application, refuses the
  duplicate; the API path was never in doubt.

- **G-29: `incidents.created_by` carried two meanings across a row's
  lifetime — "who opened it" until resolved, then overwritten to "who
  resolved it" — so the incident CSV's `created_by` column meant
  different things for open and resolved rows, and a consumer parsing
  the export could not tell which.** Split into `opened_by` and
  `resolved_by` (migration `0008`). Backfilled from ground truth, not
  from `created_by`'s current value: `open()` takes no caller and has
  only ever written the literal `'system'`, so `opened_by = 'system'`
  for every existing row regardless of status; `resolved_by` backfills
  from `created_by` only for rows already resolved. See the Changed
  section above for the resulting CSV breaking change.

### Removed

### Security

### Known issues

### Rollback

## [0.31.0] — 2026-08-11

### Added

- Migration `sql/0005_target_thresholds.sql`: `success_threshold`/
  `failure_threshold` move from `target_states` to `targets` — see
  Fixed, G-06, below. [RFC 0008](rfcs/done/008-target-thresholds-on-target.md)
  (accepted, DEC-012) moves to `rfcs/done/`, implemented.

### Changed

- `db::states::update_after_check` now takes `success_threshold`/
  `failure_threshold` as parameters instead of reading them off the
  state row — its only caller (`monitor::engine::run_scheduled_checks`)
  passes the target's values. `decide_transition` itself, and its unit
  tests, are unchanged (T-49).
- `noye_shared::Target` gains `created_by`, `updated_by`,
  `success_threshold`, `failure_threshold`. `TargetState` loses
  `success_threshold`/`failure_threshold`. `CreateTargetInput`/
  `UpdateTargetInput` gain optional `success_threshold`/
  `failure_threshold` (default 3, matching the prior schema default).

### Fixed

- **G-05: configuration import could not create a target.**
  `db::migration::upsert_target`'s `INSERT` omitted `created_by`/
  `updated_by`, both `NOT NULL` with no default — every import failed
  with `NOT NULL constraint failed: targets.created_by`, confirmed
  against real local D1. Fixed by adding the columns and always binding
  them to the *importing* caller, never a value from the document (which
  would name a user ID from another deployment) — matching FR-MIG-08's
  "equivalent to the normal creation path".

- **G-31: the default configuration export — the primary documented use
  case — could not be imported into a fresh deployment.**
  `include_users` defaults to off (FR-MIG-04: an export containing user
  emails is more sensitive than one without), but `targets.owner_id` and
  `notification_channels.owner_id` are `NOT NULL` foreign keys to
  `users(id)`, and import performed no reference validation before
  writing — a document exported with default options failed with a raw
  constraint error, not a validation report. Fixed with
  `db::migration::find_unresolvable_owners`, which resolves every
  `owner_id` referenced by an incoming target or channel against the
  destination (an existing user, or one the same payload carries) before
  any write, in both dry run and a real import, and reports every
  unresolvable reference together, e.g. *"3 targets and 1 channel
  reference users that do not exist in this deployment. Re-export with
  'Include users' enabled, or create the users first."*

- **G-22: re-importing an existing target silently destroyed its
  monitoring history.** All five `INSERT OR REPLACE` statements in
  `db/migration.rs` (targets, notification_channels,
  maintenance_windows, users, target_notifications — the original
  review counted three; corrected to five during pre-flight) resolve a
  primary-key collision by deleting the row and reinserting it, which
  fires every `ON DELETE CASCADE` declared against it. A `replace`
  import onto a live deployment silently destroyed a target's check
  results, incidents and channel attachments while reporting success.
  Fixed by converting all five to explicit `INSERT ... ON CONFLICT(...)
  DO UPDATE SET` upserts, which update in place and trigger no cascade.
  Confirmed against real local D1 with the critical fixture: a target
  carrying 2 check results, 1 open incident and 2 channel attachments,
  re-imported with `on_conflict = replace` — all four dependent-row
  counts identical before and after.

- **G-06: an imported target was not monitorable, and its
  consecutive-count thresholds were silently reset to the default (3)
  on every export/import round trip.** Import created no
  `target_states` row, so a newly imported target had no state to look
  up; and `success_threshold`/`failure_threshold` lived on
  `target_states`, not `targets`, so the configuration document (built
  from `Target`) could not carry them. Fixed per RFC 0008 (DEC-012):
  the thresholds move to `targets` (migration `0005`), and import
  creates the `target_states` row in the same operation as the target
  — exactly what `db::targets::create` does on the normal path.
  Confirmed end to end against real local D1: a real monitor tick
  (`GET /cdn-cgi/handler/scheduled`) selected and probed a freshly
  imported target, and an imported `failure_threshold = 1` produced
  `down` after exactly one failed check, not three.

  **Operator action required:** run `wrangler d1 migrations apply` to
  apply `sql/0005_target_thresholds.sql`. Existing thresholds carry
  across the migration unchanged; nothing to reconfigure.

### Removed

### Security

### Known issues

**This is the first release in which configuration import works at
all.** Reads, writes and login worked as of `0.30.0`; import — the
primary documented use case for the configuration document — did not.

Twenty-one gaps remain open in `docs/src/requirements.md` §11's
conformance register, each named below with the subject that closes it
(none are scheduled before M2b; this is the full list, not a sample):

**M2b — suppression and SLA** (subjects 11–13)
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

**M2c — incidents and schema integrity** (subjects 14–18)
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
- G-13, G-14, G-15 (subject 18): boolean/range/interval constraints,
  a consistent timestamp format, and several access-path indexes are
  absent from the schema.

**M2d — identity and OIDC** (subjects 19–20)
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

**Found, no subject assigned yet:**
- **G-41** — reading an `INTEGER` beyond `±2^53` into a typed `i64`
  field traps rather than returning an error. **Unreachable from this
  codebase**: writes already reject anything past that boundary
  (`i64_to_d1`), and no domain column approaches it — the only route in
  is direct database access, which is operator action, not a live
  hazard. See **DEC-023**.
- **G-37** — `noye-core`'s WASM test binary cannot load under Node at
  all (a `cloudflare:`-scheme import Node's ESM loader rejects before
  any test filter runs), so the crate holding the D1 access layer, the
  monitor and the audit chain has no test that exercises the Rust/JS
  boundary where its most severe defects have lived.

**Not gap-tracked, and not remediable:** `DR-MIG-02` ("a released
migration's applied effect MUST NOT change") is `Not met` as a
standing historical fact — `sql/0001_initial.sql` shipped at tag
`0.1.0` and had DDL added to it at `0.27.2`, which is the direct cause
of the now-closed G-01. That already happened; no future subject
reverses it.

### Rollback

**A code-only rollback to `0.30.0` does not restore `0.30.0`'s
behaviour — it breaks monitoring outright, in a new way.** Migration
`0005` removed `success_threshold`/`failure_threshold` from
`target_states`; `0.30.0`'s code still reads them from there. Every
`db::states::update_after_check` call — the per-check state-transition
update, once per target per interval — fails to deserialize
`target_states`, because the columns it expects no longer exist. This
is not the reoccurrence of an old, understood defect; it is a new one,
introduced by mismatching code and schema.

**Roll back the database, not only the code, or do not roll back.**

If the database is also restored to its pre-`0005` state (both code and
schema reverted together), the four gaps this release closed reoccur
exactly as before:

- **G-05**: configuration import cannot create a target again — every
  import fails with `NOT NULL constraint failed: targets.created_by`.
- **G-31**: the default export (`include_users` off) cannot be
  imported into a fresh deployment again — a raw foreign-key error, not
  a validation report.
- **G-22**: re-importing an existing target with `on_conflict = replace`
  destroys its check results, incidents and channel attachments again,
  while reporting success.
- **G-06**: an imported target is not monitorable again, and configured
  thresholds are silently reset to the default (3) on every
  export/import round trip.

**The correct response to trouble on `0.31.0` is to fix forward.** If a
rollback is unavoidable, it must include reverting the database to
before migration `0005` — a code revert alone trades a known, closed
set of defects for an unhandled deserialization failure on every
monitoring check.

## [0.30.0] — 2026-08-11

**No migration required for this release.** `0.29.0` required an operator
to run `wrangler d1 migrations apply`; M1.1 added no file to `sql/`, and
this release needs no migration step at all.

### Added

### Changed

### Fixed

- **G-36 and G-38: the service could not read from or write to D1 at
  all.** Both are the same underlying problem on opposite sides of the
  Rust/JS boundary, found and fixed two days apart, and **neither is a
  regression** — both predate every release, so this is not a
  data-layer repair to a previously working service; it is the first
  release against which the service actually works.

  - **G-36 (reads)**: every `bool`-typed field backed by a D1
    `INTEGER` column (`User.is_active`, `Target.is_disabled`,
    `CheckResult.is_success`, `MaintenanceWindow.is_active`/
    `suppress_notify`, `NotificationChannel.is_enabled`,
    `RetentionPolicy.archive_to_r2`) failed to deserialize — D1
    surfaces the column as a JS number, and `worker`'s internal
    `.unwrap()` on the deserialize result turned the mismatch into an
    uncaught Wasm trap rather than a returned error, so nothing was
    ever logged. Fixed with `bool_from_d1`, a `serde::de::Visitor`
    accepting a genuine boolean or the integer/float SQLite actually
    stores (non-zero is true; `NaN` is rejected rather than read as
    `true`), applied to all seven fields.
  - **G-38 (writes and paginated reads)**: binding an `i64` to a D1
    statement produces a JS `BigInt`, which D1's bind validation
    refuses outright (`D1_TYPE_ERROR: Type 'bigint' not supported`).
    23 binds across 10 statements in 6 modules — a target could not be
    created or updated on any path, the monitor could not record a
    check result carrying a status code, response time or TLS
    days-left (the highest-frequency write in the system), an incident
    could not be resolved, and results, incidents and audit entries
    could not be listed. Fixed with `i64_to_d1`/`opt_i64_to_d1`,
    building the JS Number directly and **rejecting rather than
    truncating** anything outside `±2^53` — a truncating cast would
    have passed every test here while silently storing a different
    number than the operator entered.

  Both confirmed against real local D1, not just unit tests — G-36's
  fix produced this project's first successful typed read against D1;
  G-38's fix produced the first completed `run_cleanup` pass, which
  archived and deleted a real eligible row.

  A third, related but unfixed finding surfaced along the way:
  **G-39**, `db/migration.rs` already avoids G-38's crash by casting
  every `i64` to `i32`, which is safe from the `BigInt` rejection but
  silently truncates — deliberately not folded into this fix; see
  `docs/src/requirements.md` §11.

- **G-39**: `db/migration.rs`'s import path converted `port`,
  `expected_status`, `tls_threshold_days`, `timeout_sec`, `retry_count`
  and `interval_minutes` via `as i32` — safe from G-38's `BigInt`
  rejection, but a silently truncating conversion. Fixed with the same
  `i64_to_d1`/`opt_i64_to_d1` used everywhere else, so the codebase
  converts integers for D1 by one rule instead of two. A boundary audit
  (`docs/src/d1-type-boundary.md`) confirmed every Rust type this
  project binds to or reads from D1 against the local D1 runtime, in
  both directions — its central finding is that integers cross exactly
  only within `±2^53` (**DEC-023**), a property of the platform, not a
  defect: writes already enforce it, and reads cannot recover a
  violation that already happened at the boundary.

- **G-42: OIDC login could not start — an outage, not a vulnerability.**
  `crypto::sha256()`, used to generate every PKCE S256 code challenge,
  annotated `crypto.subtle.digest()`'s resolved value as a `Uint8Array`
  and cast it directly — but `subtle.digest()` resolves to an
  `ArrayBuffer`, which is never `instanceof Uint8Array` in any
  conforming JS engine, so the cast could not succeed regardless of
  runtime. **Not a regression** — it predates every release. Confirmed
  under both Node and `workerd` (the actual runtime Cloudflare deploys,
  reachable locally via `wrangler dev --local`) before the fix was
  applied, so this is an observation, not an inference: **every login
  attempt failed with a 500.** It failed *closed* — `sha256()` returns
  `Result`, and the caller maps it to an error — so no weak challenge,
  predictable verifier, or bypass was ever produced; nothing was less
  secure than intended, only unusable. Fixed by casting to
  `js_sys::ArrayBuffer` first, then wrapping the result in a
  `Uint8Array` view — the fix the code's own comment already described,
  with the type annotation corrected to match. `noye-gateway`'s WASM
  test suite (13 tests across SHA-256, random-number generation,
  base64url and JWT verification) now runs in CI.

### Removed

### Security

### Known issues

**This is the first release of Noye against which the service functions.**
Reads, writes and login all worked for the first time during M1.1. None of
the four defects behind that — G-36, G-38, G-39, G-40/G-42 — was a
regression; every one predates every release, including the three already
shipped.

Twenty-five gaps remain open in `docs/src/requirements.md` §11's
conformance register, each named below with the subject that closes it
(none are scheduled before M2; this is the full list, not a sample):

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

**Found during M1.1, no subject assigned yet:**
- **G-41** — reading an `INTEGER` beyond `±2^53` into a typed `i64`
  field traps rather than returning an error. **Unreachable from this
  codebase**: writes already reject anything past that boundary
  (`i64_to_d1`, G-38's fix), and no domain column approaches it — the
  only route in is direct database access, which is operator action,
  not a live hazard. See **DEC-023**.
- **G-37** — `noye-core`'s WASM test binary cannot load under Node at
  all (a `cloudflare:`-scheme import Node's ESM loader rejects before
  any test filter runs), so the crate holding the D1 access layer, the
  monitor and the audit chain has no test that exercises the Rust/JS
  boundary where its most severe defects have lived.

**Not gap-tracked, and not remediable:** `DR-MIG-02` ("a released
migration's applied effect MUST NOT change") is `Not met` as a
standing historical fact — `sql/0001_initial.sql` shipped at tag
`0.1.0` and had DDL added to it at `0.27.2`, which is the direct cause
of the now-closed G-01. That already happened; no future subject
reverses it. Recorded here so a reader auditing migration history
knows why, not because a fix is pending.

### Rollback

**Do not roll back this release — fix forward instead.**

Reverting `0.30.0` → `0.29.0` means redeploying `0.29.0`'s code. M1.1
added **no migration** — every one of its fixes was pure code — so unlike
every rollback note before this one, there is no database-side guard
softening the reversal. **All of it reoccurs immediately, together, on a
code-only rollback:**

- **G-36**: no typed read from D1 can succeed again — listing targets,
  authenticating a user, recording a check result, evaluating a
  maintenance window, and the retention pass all trap on the first row
  of any of the seven affected fields.
- **G-38**: no write or paginated read can succeed again — a target
  cannot be created or updated, the monitor cannot record a check
  result, an incident cannot be resolved, and results, incidents and
  audit entries cannot be listed.
- **G-39**: `db/migration.rs`'s import path silently truncates six
  integer fields again — low severity in isolation, but back
  nonetheless.
- **G-40 / G-42**: OIDC login cannot start again — `sha256()` fails on
  every PKCE code-challenge computation, so every login attempt returns
  a 500.

**A deployment on `0.29.0` — or on any release before it — could not
read a row, write one, or accept a login.** That was true the whole
time; `0.30.0` is simply the first release anyone looked hard enough to
find out. Reverting does not restore a previously-working state — it
restores the same non-functioning one this release replaced. If `0.30.0`
causes trouble, the correct response is to fix forward on `0.30.0`, not
to revert to a release that has never worked.

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
