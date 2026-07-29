# 21 — Screen re-expression spike

**Milestone** M3 · **Closes** no gap · **Depends on** M2 shipped
**Branch** `feat/21-spike` · **Do this before any other M3 subject.**
**Governing artifact** — **RFC 0011** — the measurement its §"estimate" section requires before M4 is sequenced

## Why this exists

Nobody has re-expressed a mockup screen against the production UI layer.
Every statement about what M4 involves is currently speculation,
**including mine**. This subject replaces a guess with a measurement.

## Build

Re-express **the target list** (`/targets`, screen S-02) against the
mockup's design, in the existing pure-function UI layer.

It is the right candidate because it exercises most of what M4 will
touch: a filter bar, a status badge, an empty state, role-conditional
controls, and a table that becomes cards on narrow viewports.

### ⛔ Do not proceed to a second screen

One screen, then report. Proceeding without the report is how the
measurement gets skipped.

## Report

The deliverable is not the screen. Write
`rfcs/handoffs/evidence/21-spike-report.md` answering:

- What was mechanical, and what needed judgement?
- Which existing tests survived unmodified, and which needed rewriting
  because they asserted on English copy rather than structure?
- Did token reconciliation surface contrast problems?
- What would you do differently on the next twelve?

**Subject 26's internal order is sequenced from this report.** An honest
account of what was awkward is worth more than a clean one.

## Verify

No new tests. Confirm the re-expressed screen still passes every existing
test for S-02 — and record how many needed rewriting and why.

## Done

- `/targets` renders from the refreshed design
- Spike report written
- Count of rewritten tests recorded, with the reason for each

## Escalate

The spike taking materially longer than one screen's worth of work →
requirements architect, **before starting a second screen.** That is the
signal M4 needs resequencing, and it is the whole point of this subject.
