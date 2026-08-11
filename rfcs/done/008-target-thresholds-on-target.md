# RFC 0008: Move consecutive-count thresholds onto the target

**Status**: Implemented — version pending release. Built as subject 10,
alongside subjects 08/09 (the configuration-import repair this RFC names
as its prerequisite). Landed after M1.1 (`0.30.0`) shipped, as part of
M2, whose version is still provisional (`0.31.0` per `ROADMAP.md`) —
DEC-007 decides the real number at release time, so this deliberately
does not assert one yet, unlike this RFC's own original "Implementation
target: 0.29.0" below, which the M1.1 diversion has since made incorrect.
**Author**: nabbisen
**Last updated**: 2026-07-28
**Related ROADMAP item**: none — this is a correctness fix surfaced by gap G-06
**Estimated size**: small (~1 day, inside Phase 2)
**Implementation target**: 0.29.0, alongside the configuration-import repair

---

## Summary

`success_threshold` and `failure_threshold` currently live on
`target_states`. They are not state; they are per-target configuration.
Move them to `targets`, where every other decision criterion already
lives.

This is the direct cause of gap G-06's second limb — thresholds are lost
in an export/import round trip — and it cannot be fixed cleanly while
they sit on the state row.

## Background

`sql/0001_initial.sql` puts eight columns on `target_states`:

| Column | Nature |
|---|---|
| `current_status` | state |
| `consecutive_successes` | state |
| `consecutive_failures` | state |
| `last_checked_at` | state |
| `last_status_change_at` | state |
| `last_notification_at` | state |
| **`success_threshold`** | **configuration** |
| **`failure_threshold`** | **configuration** |

Every other decision criterion — `expected_status`, `body_contains`,
`tls_threshold_days`, `timeout_sec`, `retry_count`, `interval_minutes` —
is on `targets`. FR-TGT-03 groups thresholds with them: *"A target MUST
carry decision criteria appropriate to its type."*

The split has three consequences today:

1. **DR-ENT-04 fails.** The configuration document is built from
   `Target`, so thresholds are not exported, and an import round trip
   silently resets them to the schema defaults of 3 and 3. A target
   deliberately configured to fail over after one check comes back
   failing over after three.
2. **The import repair gets harder.** Fixing G-06 means creating a
   `target_states` row on import. If thresholds live there, the
   configuration document must carry them somewhere — either a parallel
   `target_states` collection, which exports state as configuration, or
   an awkward embedding in `Target` that does not match the table it is
   read from.
3. **The state row cannot be treated as derived.** A `target_states` row
   is otherwise fully reconstructible: delete it and monitoring
   rebuilds it from the next check. Two configuration columns make it
   load-bearing.

## Design

### Schema

Migration `0005`:

1. `ALTER TABLE targets ADD COLUMN success_threshold INTEGER NOT NULL DEFAULT 3;`
2. `ALTER TABLE targets ADD COLUMN failure_threshold INTEGER NOT NULL DEFAULT 3;`
3. Copy existing values across by `target_id`.
4. Rebuild `target_states` without the two columns, per the standard
   SQLite table-rebuild procedure.

Step 4 is optional in the sense that leaving dead columns would work.
It should still be done: a duplicated configuration value with no
defined authority is exactly the ambiguity this RFC exists to remove.

### Shared type

`noye_shared::Target` gains `success_threshold` and `failure_threshold`.
`TargetState` loses them. Export and import then carry them with no
further work, because both are driven by `Target`.

### Read path

`decide_transition` already takes thresholds as arguments and is a pure
function (FR-MON-07). Only its caller changes — reading from the target
rather than the state row. The transition logic itself is untouched, and
its unit tests should continue to pass unmodified. **If they do not,
something has been changed that this RFC does not authorise.**

### Constraint

Phase 4 adds range constraints (DR-INT-02). Include the thresholds:
`CHECK (success_threshold BETWEEN 1 AND 10)` and the same for
`failure_threshold`. A threshold of 0 would mean "transition on no
evidence" and must not be representable.

## Requirements

Satisfies DR-ENT-04, which currently reads `Not met`. Brings thresholds
under FR-TGT-03's "decision criteria" grouping, which is where the
requirement text already implies they belong.

No requirement text changes. This RFC corrects an implementation that
diverged from the requirement, not the requirement itself.

## Test plan

- Export → import → export reproduces non-default thresholds exactly.
- An imported target with `failure_threshold = 1` transitions to `down`
  after one failed check, not three.
- Existing `decide_transition` unit tests pass **unmodified**.
- After migration, no threshold column remains on `target_states`.
- Values configured before the migration survive it.

## Why this is treated differently from RFC 0007

Both were surfaced by the same review. RFC 0007 (atomic audit writes)
was deferred out of its repair phase; this one is recommended *into*
it. The distinction is deliberate:

| | RFC 0007 | RFC 0008 |
|---|---|---|
| Shape | Cross-cutting refactor of ~8 `db::*` modules to return statements instead of executing them | Two columns move between two tables |
| Effort | ~3 days | ~1 day |
| Relationship to the repair | Independent — the repair is correct without it | **Prerequisite** — the import fix is incomplete without it, or needs a design that is thrown away later |
| Cost of deferring | The weaker guarantee is honestly documented | The import path is built twice |

Deferring RFC 0008 does not save the work; it schedules it twice.

## Security considerations

None directly. One indirect benefit: with thresholds on the target, the
`target_states` row becomes fully derived, so a corrupted or missing
state row can be rebuilt from the next check rather than needing
restoration from backup.

## Out of scope

- `consecutive_successes` / `consecutive_failures` counters, which are
  genuinely state and stay where they are.
- Per-type default thresholds. The current flat default of 3 is
  adequate, and varying it by probe type is speculative until an
  operator asks.
- Any change to `decide_transition` itself.
