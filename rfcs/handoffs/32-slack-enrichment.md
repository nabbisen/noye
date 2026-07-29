# 32 — Slack payload enrichment

**Milestone** M5 · **Satisfies** FR-NTF-12
**Implements** [RFC 0006](../proposed/006-slack-payload.md)
**Branch** `rfc-0006-slack` · **Depends on** M4 shipped
**Governing artifact** — **RFC 0006**

## ⚠️ Read `crates/core/src/notify.rs` first

**A Block Kit adapter already ships** — per-status colour, emoji, a
mrkdwn section and a context block, at `notify.rs:171-183`.

`ROADMAP.md` and RFC 0006 both said Slack receives the same generic JSON
as webhooks. That was false since before v0.27.2 and was corrected on
2026-07-28. Anyone picking this up on the old text would begin by
reimplementing working code.

**What is deferred is enrichment, not introduction.**

## Build

Per RFC 0006, as enrichment of the existing adapter:

- A header block
- Structured fields — target, cause, duration
- A deep link back into the interface

### Do not

**Do not change the generic webhook payload.** Its six fields —
`title`, `body`, `status`, `target_name`, `target_host`, `timestamp` —
are a **stable external contract** parsed by every existing integration.
Changing or removing one is a breaking change to all of them.

This subject touches the Slack path only.

## Verify

| # | Test | Type |
|---|---|---|
| T-151 | The Slack payload carries a header block, structured fields and a deep link | **must fail first** |
| T-152 | The generic webhook payload still carries exactly its six fields, unchanged | **guard — critical** |
| T-153 | Per-status colour and emoji are unchanged | guard |

## Done

- All three tests pass
- RFC 0006 → `rfcs/done/`, `Status: Implemented (1.0.0)`, inbound links fixed
- `docs/src/requirements.md`: FR-NTF-12 → `Implemented`
