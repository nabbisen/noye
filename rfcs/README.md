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
```

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
