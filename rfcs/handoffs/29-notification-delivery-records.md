# 29 — Notification delivery records

**Milestone** M5 · **Closes** G-18 · **Satisfies** FR-NTF-13
**Branch** `feat/29-delivery-records` · **Depends on** M4 shipped
**Governing artifact** — Gap **G-18** (§11)

## The defect

Delivery outcomes go to `console_log!` only. An operator cannot answer
*"was this incident notified?"* after the fact — which is exactly the
question that comes up in the post-incident review the audit trail exists
to serve.

## Build

A `notification_deliveries` table keyed on incident, channel and attempt,
recording outcome, timestamp, and the error where there was one. Written
on every dispatch attempt, including test sends.

Surface it on the incident detail screen from subject 27.

### Do not

Do not let a failure to write a delivery record interrupt monitoring or
incident recording (NFR-REL-01, FR-NTF-14). Same discipline as DEC-011:
record the failure, do not propagate it into the monitoring path.

**This is the risk in this subject.** It adds a write to the one path the
requirements insist must never block monitoring. Adding observability to
it is exactly where that guarantee gets broken by accident.

## Verify

| # | Test | Type |
|---|---|---|
| T-139 | Every dispatch attempt produces a row, success or failure | **must fail first** |
| T-140 | A failing channel produces a row recording the failure | **must fail first** |
| T-141 | …and does **not** prevent the state update or the incident record | **guard — critical** |
| T-142 | A failure to write the delivery row itself does not interrupt monitoring | **guard — critical** |
| T-143 | A test send produces a delivery row like any other dispatch | **must fail first** |

T-141 and T-142 matter more than T-139.

## Done

- All five tests pass; three baseline failures captured
- `docs/src/requirements.md`: FR-NTF-13 → `Implemented`, G-18 struck
