# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Note: changelog entries begin with the next release. Earlier history is
recorded in the project's release tags and the [docs/src/requirements.md](docs/src/requirements.md)
coverage table.

## [Unreleased]

### Added

- Migration `sql/0003_audit_retention_exemption.sql`, removing the
  seeded `audit_logs` retention policy row (idempotent). First
  migration after `0002`'s retirement (DEC-010) — the numbering gap
  is intentional.

### Changed

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

### Removed

### Security

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
