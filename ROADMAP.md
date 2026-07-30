# Noye Roadmap

Where the project is going, in order — and what has been deliberately
set aside, with the reasoning, so a future maintainer can judge whether
it is still wanted.

> **Detailed specifications for the priority items live in
> [`rfcs/`](rfcs/).** This roadmap stays high-level; each RFC takes one
> entry to implementer-ready depth. See [`rfcs/README.md`](rfcs/README.md)
> for the index, the ordering, and the workflow contract.

> **No dates and no effort estimates here, by intent.** The order is the
> commitment; the calendar is not. Work orders in
> [`rfcs/handoffs/`](rfcs/handoffs/) carry the executable detail.

---

## Release plan

Each milestone is independently shippable. Order is a dependency order,
not a preference — every milestone rests on the one before it.

| # | Version | Theme | Phases |
|---|---|---|---|
| **M0** | 0.28.0 | **Provisionable** | 0 |
| **M1** | 0.28.1 | **Audit trail trustworthy** | 1 |
| **M2** | 0.29.0 | **Conformant and deployable** | 2, 3, 4 |
| **M3** | 0.30.0 | **Design frozen** | 5 |
| **M4** | 0.40.0 | **Interface integrated** | 6 |
| **M5** | 1.0.0 | **Service complete** | 7 |

**M2 is the milestone that matters most.** It is the first point at
which Noye provisions from empty, deploys, and reports figures that
match their own on-screen explanations. Everything before it is repair;
everything after it is improvement.

### Full task inventory

Every phase, every gap, in dependency order. Gap identifiers index
[`docs/src/requirements.md`](docs/src/requirements.md) §11; work orders
live in [`rfcs/handoffs/`](rfcs/handoffs/).

| Phase | Milestone | Closes | Work order | Ready? |
|---|---|---|---|---|
| **0** — stop the bleeding | M0 | G-01 migration set unapplyable · G-20 retention deletes more than it archives · G-21 shipped config is the development one · G-24 archive layout · G-32 the CI vulnerability scan has never run · G-33 the CI format/lint/check job has never run · G-34 the release archive is built from the working directory | [index](rfcs/handoffs/README.md) | **yes** |
| **1** — audit trail | M1 | G-04 retention deletes audit rows · G-03 system actor unwritable · G-26 write failures discarded · G-30 writer and verifier disagree on chain order | [index](rfcs/handoffs/README.md) | **yes** |
| **2** — configuration import | M2 | G-05 provenance columns · G-06 no state row, thresholds lost · G-22 replace destroys history · G-31 default export not importable | [index](rfcs/handoffs/README.md) | **yes** — RFC 0008 |
| **3** — suppression and SLA | M2 | G-07 flags ignored · G-08 scope ambiguity · G-09 substring tag match · G-27 LIKE wildcards · G-12 SLA denominator | [index](rfcs/handoffs/README.md) | **yes** — DEC-013 |
| **4** — incidents and schema | M2 | G-10 no duration on auto-resolve · G-11 duplicate open incidents · G-28 unreachable target states · G-29 `created_by` overloaded · G-13 missing constraints · G-14 timestamp formats · G-15 missing indexes · G-16 case-sensitive identity · G-17 unreachable incident state · G-19 no OIDC endpoint overrides | [index](rfcs/handoffs/README.md) | **yes** — DEC-014 |
| **5** — design freeze | M3 | No gaps. Decides interface scope and resolves D-3 | [index](rfcs/handoffs/README.md) | **yes** — DEC-015, DEC-016 |
| **6** — interface integration | M4 | No gaps. Re-expresses the accepted screens | [index](rfcs/handoffs/README.md) | **yes** — RFC 0011 |
| **7** — service completion | M5 | G-18 no delivery records · G-23 inline tests · G-24 packaging and language · G-25 rotten cross-references | [index](rfcs/handoffs/README.md) | **yes** |

All thirty-three gaps are assigned. (G-02 is intentionally absent — the
review's second finding was the absence of multi-tenant structure, which
was a product question, resolved as [DEC-008](docs/src/decision-log.md).)

### Decisions still open

| ID | Question | Needed by |
|---|---|---|
| **D-5** | Does the release archive carry `Cargo.lock`? | **Subject 03d** — no longer deferrable: `git archive` cannot exclude a tracked file without extra machinery |

One decision remains, and it is **no longer deferrable**: `git archive`
cannot exclude a tracked file without extra machinery, so subject 03d
cannot choose a `Cargo.lock` default on the owner's behalf. It blocks
distributing any release archive, though not tagging.

Resolved: D-1 and the role model ([DEC-008](docs/src/decision-log.md),
DEC-009), D-A (DEC-011), RFC 0008 (DEC-012), D-2 (DEC-013), D-4
(DEC-014), D-B (DEC-015), D-3 (DEC-016).

### A note on M2 as a candidate 1.0

Shipping M2 as 1.0 and treating M3–M5 as the 1.x line is a defensible
reading of this project's own first principle — minimum features for
safety and transparency. A correct, auditable, deployable monitor is
that product. The interface refresh is an improvement to it. This is
recorded as an option, not a decision.

---

## Deferred

## UI / theme

### Manual theme toggle (light / dark / system)

**RFC**: [0001](rfcs/proposed/001-manual-theme-toggle.md).

**Status**: deferred (since 0.23.0).

**Why deferred**: 0.23.0 introduced a token-based design system with
both light and dark presets. The active theme is currently selected by
`prefers-color-scheme` only — the OS-level setting wins. A manual
toggle would let users override the OS preference within a Noye session
(e.g. an admin who runs a dark-themed OS but wants Noye in light mode
during a daytime briefing).

**Suggested implementation when picked up**:

- Persist the preference in a cookie (`noye_theme=light|dark|system`,
  `Path=/`, `Max-Age=31536000`, `Secure` in production, `HttpOnly`
  unset because the client needs to read it).
- Read the cookie in the `wrap()` helper and write a `data-theme`
  attribute on `<html>`. Add a corresponding `[data-theme="light"]`
  selector in `style.rs` that mirrors the `prefers-color-scheme: light`
  block.
- Render a small toggle in the user-info chip (top-right) — three-state
  button cycling system → light → dark → system.
- The cookie path keeps the toggle JS-free if needed (a `<form
  method="POST" action="/me/theme">` submission could update it
  server-side); see if there's appetite for a no-JS path.

**Why not now**: would have stretched Phase A beyond its scope. Phase
A was focused on the token system itself — once the tokens are stable,
adding a third theme branch and a UI control is straightforward
incremental work.

### High-contrast mode preset

**RFC**: [0005](rfcs/proposed/005-high-contrast-theme.md).

**Status**: deferred.

**Why deferred**: WCAG AAA (7:1 body, 4.5:1 large) is achievable with a
small token override but adds maintenance burden. None of the current
operators have requested it and the AA baseline already covers the
disability-discrimination compliance bar in most jurisdictions.

**Suggested implementation when picked up**: add a `[data-theme=
"high-contrast"]` token preset that pushes text colors closer to pure
black/white and bumps border-strong contrast. Pin the new pairs in
`contrast.rs::tests::critical_pairs_meet_aa` against the AAA threshold.

## Operations infrastructure

### Cargo.lock commit + GitHub Actions CI + cargo-audit

**Status**: ✅ working since 2026-07-29. Scaffolded in 0.27.0, but **CI
had never once gone green** on any branch until M0 fixed two jobs that
never executed. Run `30460161440` is the first fully green run in the
project's history.

`Cargo.lock` is committed and the workflow exists. Two of its jobs never
executed:

- **`cargo audit`** invoked `cargo audit --locked`; cargo-audit rejects
  the flag and exits before scanning. RUSTSEC-2026-0190 went undetected
  for a month. Gap **G-32**, fixed by
  [subject 03b](rfcs/handoffs/03b-ci-dependency-scan.md).
- **"Format, lint, check"** installed the toolchain with
  `--component rustfmt clippy`, space-separated where a comma is
  required, so `clippy` parsed as a toolchain name and the job died at
  its first step. Format, Clippy and Check have never run. Gap **G-33**,
  fixed by [subject 03c](rfcs/handoffs/03c-ci-toolchain-install.md).

Both were introduced in `5de978d`, the 0.27.2 baseline, and both were
invisible because the controls were verified by reading configuration
rather than by observing a run. Both fixes are confirmed by real runs in
**both directions** — passing on a clean tree, and failing on a
deliberately introduced violation, which a clean-tree pass alone would
not have proven.

Note SEC-006: `cargo-audit` is advisory on pull requests, so a PR
introducing a vulnerable dependency fails the job without failing the
run. See `docs/src/development.md#continuous-integration`.

### Cloudflare Logs export (audit-log mirror)

**RFC**: [0002](rfcs/proposed/002-audit-log-mirror.md).

**Status**: deferred (operator-side configuration).

**Why deferred**: the in-D1 hash chain detects tampering, but a
wholesale `DROP TABLE audit_logs` leaves nothing to verify against. A
log-shipping mirror to an off-D1 destination is the recovery path. This
is configured at the Cloudflare level rather than in Noye code.

**Suggested implementation when picked up**: document a
`docs/operations/audit-log-mirror.md` runbook covering Logpush
configuration, retention guidance, and how to use the mirrored stream
to repair a corrupted `audit_logs` table.

### Atomic audit writes

**RFC**: [0007](rfcs/proposed/007-atomic-audit-writes.md).

**Status**: deferred (since 0.28.1).

**Why deferred**: 0.28.1 closed FR-AUD-08 by the *surface and complete*
route — an audit write failure leaves the mutation applied and reports a
warning to the operator (DEC-011). The stronger property, "no change
occurs without a record of it", needs the business mutation and the
audit insert to share a transaction. D1's `batch()` provides that, but
it requires roughly eight `db::*` modules to return prepared statements
instead of executing them, which is a cross-cutting refactor that did
not belong inside a repair phase.

**Notes**: this is a behaviour change visible at the API contract — an
audit failure would begin returning an error instead of a 200 with a
warning — so the external design must be amended before implementation.
It does **not** relax the single-writer constraint from DEC-004; Queue
fan-out remains a separate prerequisite.

## Feature

### Workers Queue fan-out for Cron monitor

**Status**: deferred (scale).

**Why deferred**: Noye's monitor engine processes targets serially
within one Cron tick; for the current scale (~ 数百 targets) this
finishes well within the one-minute window. Past ~1000 targets the
fan-out becomes necessary to keep latency bounded.

**Notes**: the audit-log hash chain is currently single-writer; if
fan-out is added, audit-log writes need a Durable Object (or an
external serialization point) to avoid chain forks.

### Turnstile activation

**RFC**: [0003](rfcs/proposed/003-turnstile-activation.md).

**Status**: scaffolded but not wired up.

**Why deferred**: the Cloudflare Turnstile integration code exists
under `gateway::auth::turnstile` but the UI and rate-limit don't
require it today. Activation is gated on observing actual abuse against
`/auth/login` past what the IP rate limit (10/min, 50/hour) can absorb.

### Slack-specific notification payload formatting

**RFC**: [0006](rfcs/proposed/006-slack-payload.md).

**Status**: deferred.

**Why deferred**: *(corrected 2026-07-28 — the previous text stated that
Slack receives the same generic JSON as the Webhook channel. That has
been false since before 0.27.2.)* Slack already receives a Block Kit
document with per-status colour, emoji, a mrkdwn section and a context
block. What is deferred is **enrichment**: a header block, structured
fields, and a deep link back into the interface. An implementer picking
this up should read `crates/core/src/notify.rs` first — the adapter
exists.

### Failed-login audit recording

**RFC**: [0004](rfcs/proposed/004-failed-login-audit.md).

**Status**: deferred.

**Why deferred**: the OIDC callback only records *successful* login
events to `audit_logs`. Failed attempts log to `console_error!` but
don't appear in `/me/security` recent-logins or in the chain. Adding
them would be straightforward but requires deciding what to attribute
the row to (the attempted email may not match a real user; the failure
may be earlier than that).

### HTML / multipart email bodies

**Status**: deferred.

**Why deferred**: notification emails are plain text today, which is
adequate for short DOWN/UP alerts. `mail-builder` makes it
straightforward to add HTML alternatives, but no operator has asked
for it.
