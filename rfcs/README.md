# Noye RFCs

This directory holds **detailed specifications** for the items deferred
in [`ROADMAP.md`](../ROADMAP.md). Each RFC takes a roadmap entry from a
one-paragraph deferral note to a level of detail an implementer can
work from without having to recover the design choices themselves.

## Lifecycle and layout

This directory follows the RFC lifecycle policy in
[`done/000-rfc-lifecycle-policy.md`](done/000-rfc-lifecycle-policy.md).
The **folder is the source of truth for an RFC's state**; the `Status`
field inside each file mirrors its folder.

```
rfcs/
  README.md            <- this index
  proposed/            <- open for review; implementer should not start yet
  done/                <- implemented / in effect (historical record)
  archive/             <- withdrawn or superseded
  handoffs/            <- implementation companions; NOT a lifecycle state
```

`handoffs/` is the one subdirectory that does **not** carry lifecycle
meaning. It holds execution packages — what to build and how to verify
it — per the policy's "Companion handoffs" section. An RFC's state is
never inferred from anything in there.

**A handoff never overrides an RFC.** If handoff work uncovers a design
conflict, the RFC changes first and the handoff follows. See
[`handoffs/README.md`](handoffs/README.md).

### What may govern a handoff

The organisation policy frames handoffs as derived from accepted RFCs.
This project also derives them from three other artifacts, approved by
the human owner as
[DEC-018](../docs/src/decision-log.md#dec-018):

| Governing artifact | When | Subjects |
|---|---|---|
| An **RFC** | New design or deferred feature work | 6 |
| A **conformance-gap entry** in `requirements.md` §11 | Remediating a defect the v0.27.2 review found and verified against source | 31 |
| A **decision record** | The work exists because a decision was taken | 3, overlapping |
| A **requirement** directly | Closing a `Deferred` requirement with no separate design question | 1 |

Every subject states which in a **Governing artifact** field, so
traceability is followable without inference. A gap entry is *equivalent*
to an RFC for this purpose — it carries the problem, the requirement
violated, the consequence and the remediation order, and was verified
against source when written.

An RFC changes state by moving between folders; its `Status` field is
updated in the same change. Numbers are assigned at creation and never
reused or renumbered.

## How RFCs are written

An RFC covers, at minimum:

- **Summary** - one-paragraph statement of intent.
- **Design** - concretely, what changes (files, data, API, UI).
- **Out of scope** - what the RFC explicitly does not cover.

When a topic is medium-sized or larger, the RFC also adds
**Requirements**, **Test plan**, and **Security considerations**
sections. The default writing language is English, matching the rest of
`docs/src/`.

## Order

The sequence RFCs should be taken in, and the milestone each is
attached to. A number here is a *position*, not a date — see
[`ROADMAP.md`](../ROADMAP.md) for the milestone definitions.

| Order | RFC | Milestone | Why here |
|---|---|---|---|
| 1 | [0008](proposed/008-target-thresholds-on-target.md) — thresholds on the target | **M2** | Accepted (DEC-012). A prerequisite of the configuration-import repair, not an independent improvement: without it the configuration document cannot carry thresholds and the import path is built twice |
| 2 | [0010](proposed/010-incident-acknowledgement.md) — incident acknowledgement | **M2** | Decides D-4. Recommends *removing* the unreachable state rather than implementing it. Must land in Phase 4's constraint migration or it costs a second table rebuild |
| 3 | [0009](proposed/009-multilingual-interface.md) — multilingual interface | **M3** | Decides D-3. Externalisation is far cheaper before the interface work of M4 than after; deciding late means touching every screen twice |
| 4 | [0011](proposed/011-interface-integration.md) — interface integration | **M3 / M4** | Defines what is adopted from the UI mockup and how. Without it, "integrate the mockup" has no scope and an incompatible stack |
| 5 | [0007](proposed/007-atomic-audit-writes.md) — atomic audit writes | after M2 | Strengthens a guarantee the audit remediation deliberately left at "surface and complete" (DEC-011). Cross-cutting; wants a phase of its own |
| 6 | [0003](proposed/003-turnstile-activation.md) — Turnstile activation | M5 | Smallest of the remaining feature RFCs; the scaffold already exists |
| 7 | [0004](proposed/004-failed-login-audit.md) — failed-login audit | M5 | Closes FR-AUTH-10. Depends on the audit actor model settled in M1 |
| 8 | [0006](proposed/006-slack-payload.md) — Slack payload enrichment | M5 | **Read `crates/core/src/notify.rs` first** — a Block Kit adapter already ships. This is enrichment, not introduction |
| 9 | [0001](proposed/001-manual-theme-toggle.md) — manual theme toggle | after M4 | Touches the token system; cheapest once the interface work has settled |
| 10 | [0005](proposed/005-high-contrast-theme.md) — high-contrast preset | after 0001 | Depends on the theme mechanism 0001 introduces |
| 11 | [0002](proposed/002-audit-log-mirror.md) — audit-log mirror | when required | Operator-side Cloudflare configuration rather than product code. Waits on an external-retention requirement |

Four ordering constraints are hard:

- **0008 before the M2 import work** — the document cannot carry
  thresholds without it
- **0010 inside Phase 4's constraint migration** — reopening a CHECK
  constraint afterwards costs a second table rebuild
- **0009 and 0011 before M4** — both determine what M4 builds
- **0001 before 0005** — high contrast needs the theme mechanism

The rest is judgement and may be resequenced.

Three RFCs carry an **open decision** rather than deferred work: 0009
(D-3), 0010 (D-4) and 0011 (D-B). Each must be accepted or rejected —
leaving one open is the state the RFC lifecycle policy calls silent
withdrawal.

## Index

### Implemented (`done/`)

| # | Title | Status |
|---|---|---|
| 000 | [RFC lifecycle policy](done/000-rfc-lifecycle-policy.md) | Implemented |

### Proposed (`proposed/`)

| # | Title | ROADMAP item | Size |
|---|---|---|---|
| 001 | [Manual theme toggle (light / dark / system)](proposed/001-manual-theme-toggle.md) | UI / theme | medium |
| 002 | [Cloudflare Logs export - audit-log mirror](proposed/002-audit-log-mirror.md) | Operations infrastructure | medium |
| 003 | [Turnstile activation on `/auth/login`](proposed/003-turnstile-activation.md) | Feature | small |
| 004 | [Failed-login audit recording](proposed/004-failed-login-audit.md) | Feature | medium |
| 005 | [High-contrast theme preset](proposed/005-high-contrast-theme.md) | UI / theme | small |
| 006 | [Slack-specific notification payload](proposed/006-slack-payload.md) | Feature | small-medium |
| 007 | [Atomic audit writes](proposed/007-atomic-audit-writes.md) | Operations infrastructure | medium |
| 008 | [Move consecutive-count thresholds onto the target](proposed/008-target-thresholds-on-target.md) | — (correctness, gap G-06) | small |
| 009 | [Multilingual interface](proposed/009-multilingual-interface.md) | Phase 5 — decides D-3 | medium |
| 010 | [Incident acknowledgement](proposed/010-incident-acknowledgement.md) | Phase 4 — decides D-4 | small |
| 011 | [Interface integration from the UI mockup](proposed/011-interface-integration.md) | M3 / M4 — decides D-B | large |

### Archived (`archive/`)

None yet.

## Topics intentionally not covered yet

Two ROADMAP entries have no RFC at this point:

- **Workers Queue fan-out for Cron monitor** - large, scale-driven
  rework. Not needed until target count crosses ~100; deferring the
  RFC until the requirement is concrete avoids speculative design.
- **HTML / multipart email bodies** - small but no operator has asked
  for it. The existing `mail-builder` integration makes this an
  on-demand implementation when the need surfaces.

When either graduates from "deferred speculation" to "actual upcoming
work," an RFC should be added to `proposed/`.

## Workflow

1. Pick an RFC from `proposed/`, read it end-to-end, surface any open
   questions.
2. Implement on a branch named `rfc-NNNN-short-name`.
3. When the work ships, **move the file from `proposed/` to `done/`**
   and update its `Status` field to `Implemented (X.Y.Z)` in the same
   change.
4. Note the implementing release in [`CHANGELOG.md`](../CHANGELOG.md),
   and update [`ROADMAP.md`](../ROADMAP.md) so the entry moves out of
   "deferred." If the implementation diverged from the RFC, amend the
   RFC so the record matches what shipped.
5. To drop an RFC instead, **move it to `archive/`** with a one-line
   reason in its `Status` field (`Withdrawn - ...` or
   `Superseded by RFC NNNN`).

When an RFC moves folders, run a quick `grep -rl 'NNNN-slug' .` from the
repo root and fix any inbound links in the same change.

## Numbering

Numbers are assigned in monotonic order at creation time and never
reused. An RFC that is withdrawn or superseded keeps its number; only
its folder and `Status` change. (RFC 000 uses three digits from the
lifecycle policy's own convention; the project's feature RFCs use four
digits starting at 001. Both are stable and are not renumbered.)
