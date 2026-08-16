# RFC 0010: Incident acknowledgement

**Status**: Withdrawn — acknowledgement removed per DEC-014 (subject 17, M2c-2)
**Author**: nabbisen
**Last updated**: 2026-08-15
**Related ROADMAP item**: Phase 4 — incidents and schema; resolved open decision D-4
**Estimated size**: small
**Implementation target**: 0.29.0 (Phase 4) if accepted; removal otherwise

---

## Summary

`incidents.status` accepts `'acknowledged'`. No code path produces it,
no query reads it, and no interface offers it. This RFC forces the
choice: **implement acknowledgement, or remove the value.** Leaving an
unreachable state in a CHECK constraint invites a future contributor to
assume it means something.

## Background

`sql/0001_initial.sql:81`:

```sql
status TEXT NOT NULL CHECK (status IN ('open', 'resolved', 'acknowledged'))
```

Gap G-17. The requirements glossary is explicit that incident states are
**Open** and **Resolved**, and that "Active", "Closed" and "Pending"
must not be used — terminology is normative there precisely because
operators misread state names. A third value that the system never
produces sits awkwardly against that.

Phase 4 adds range and value constraints across the schema. Whatever is
decided here should land in the same migration, because reopening a
CHECK constraint afterwards costs a second table rebuild.

## The case for implementing it

Acknowledgement answers a question Open/Resolved cannot: *someone has
seen this and is working on it.* Without it, an operator looking at the
incident queue cannot distinguish an outage nobody has noticed from one
being actively handled — and in a team of any size, that is the
difference between two people duplicating work and neither starting.

## The case against

Three arguments, and they are not weak:

1. **The premise is a handful of operators.** §2.2 targets teams where
   "has anyone seen this" is answered by asking. Acknowledgement earns
   its place when the queue is bigger than the team's ability to talk to
   each other.
2. **P-1.** Only what the requirements call for is implemented. No
   requirement calls for acknowledgement; the value appears to be
   schema-level speculation that was never designed.
3. **It is not free.** Doing it properly means an acknowledged-at
   timestamp, an acknowledging actor, an audit action type, a defined
   interaction with notification suppression (does acknowledging silence
   a re-notification?), and a UI affordance on the incident queue. That
   is a feature, not a constraint edit.

Point 3 is the substantive one. The interaction with suppression in
particular is a design question with no obvious answer, and the
requirements are silent on it.

## Recommendation: remove the value

Delete `'acknowledged'` from the CHECK constraint in Phase 4's
constraint migration. Record the reasoning in the decision log with
re-evaluation criteria, so the option is visibly available rather than
silently gone.

**Re-evaluate when** the incident queue routinely holds more open
incidents than the team can hold in their heads, or when more than one
person is expected to work the queue concurrently without talking. At
that point acknowledgement should be designed against a stated
requirement — with its suppression interaction settled — rather than
retrofitted onto a constraint value that happened to survive.

This is a removal that keeps the door open, not a rejection of the idea.

## Design, if implemented instead

Recorded so that accepting this RFC does not require redesigning it.

### Schema

- `acknowledged_at TEXT`, `acknowledged_by TEXT` on `incidents`.
- `status` keeps `'acknowledged'`.
- The partial unique index from Phase 4 (`at most one open incident per
  target`) must cover **both** `open` and `acknowledged`, or an
  acknowledged incident stops blocking a duplicate:
  `WHERE status IN ('open','acknowledged')`.

### Behaviour

- Any authenticated user who can see an incident may acknowledge it;
  resolution stays admin-only. Acknowledgement is an observation, not a
  change to the system.
- Acknowledgement does **not** alter check policy, target health, or
  the SLA figure. The interface must say so, in the same way the manual
  resolution dialog already does.
- Recovery resolves an acknowledged incident exactly as it resolves an
  open one.
- **Notification interaction, which must be decided explicitly:** the
  recommended answer is that acknowledgement changes nothing about
  notification, because Noye notifies on transitions only and never
  re-notifies. If re-notification is ever added, acknowledgement becomes
  its natural suppressor — but that is that feature's problem.

### Interface

Incident queue shows three groups, in order: open, acknowledged,
resolved. The acknowledged group carries the actor and time. Colour is
not the distinguishing signal (NFR-A11Y-03).

### Audit

A new `action_type` of `acknowledge`, recorded with actor and incident
identifier. The audit view renders unknown action types verbatim
already, so this needs no display change.

## Requirements

**If removed:** FR-INC-10 moves to `Implemented`; G-17 is struck.

**If implemented:** FR-INC-10 still moves to `Implemented` — the state
set would then contain only states the system produces. New requirements
would be needed for the acknowledgement behaviour itself, and the
glossary in §3 would need amending, since it currently states that
incident states are Open and Resolved.

## Test plan

**If removed:** an insert with `status = 'acknowledged'` is rejected;
existing rows are unaffected because none exist.

**If implemented:** acknowledging sets timestamp and actor; an
acknowledged incident still blocks a second open incident on the same
target; recovery resolves it; acknowledgement writes an audit row;
acknowledgement does not change the SLA figure.

## Out of scope

- Assignment or ownership of an incident. A different concept, and one
  that carries an on-call model the project has ruled out (DEC-009).
- Re-notification or escalation. Explicit non-goals in §2.4.
- Snooze or timed suppression of an individual incident.
