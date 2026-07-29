# 13 — SLA excludes suppressed time from the denominator

**Milestone** M2 · **Closes** G-12 · **Satisfies** FR-SLA-05, FR-SLA-09
**Branch** same as subjects 11–12 · **Depends on** 11
**Governing artifact** — Gap **G-12** (§11)

## The defect

`crates/core/src/stats.rs:194-203` computes both ratios against
`window_seconds`. Excluded time is removed from the measured outage but
**not from the denominator**.

That answers *"ignore outages during maintenance"*. The requirement, and
the explanation printed next to the figure, is *"maintenance time did not
happen for SLA purposes"* — a different number.

The figure labelled "SLA uptime" is not computed the way the adjacent
text describes.

## Build

`maintenance_seconds` is already computed. The change is arithmetic:

```rust
let effective_window = (window_seconds - excluded_seconds).max(0);
let sla_uptime_ratio = if effective_window > 0 {
    ((effective_window - sla_downtime_seconds) as f64 / effective_window as f64)
        .clamp(0.0, 1.0)
} else {
    // Whole window excluded — see below.
};
```

**The zero case is not 100%.** If the entire reporting window was
excluded there is no measured availability to report. Return *not
applicable*, rendered as an em dash — the way `mttr_seconds` already
behaves when no incident resolved in the window. Reporting 100% would be
a claim about a period in which nothing was measured.

**CSV column rename.** `/api/stats/sla.csv` column 9 is
`maintenance_seconds`. Under DEC-013's split that quantity is "time
excluded from SLA", no longer the same thing as "time in a maintenance
window". Rename to `excluded_seconds`. **This is a breaking change to
external interface I-08** and needs a version note in `CHANGELOG.md` and
a migration note for anyone parsing the export.

### ⛔ Do not implement this from the UI/UX deck

`noye_uiux_review_support-0.27.1.pdf` slide 9 states
*"SLA = (total time − incident time) / total time"* alongside
*"maintenance windows are excluded from the denominator"*. Those
contradict each other, and the formula shown is exactly the defect you
are fixing. The specification is `docs/src/requirements.md` §5.9.

## Verify

| # | Test | Type |
|---|---|---|
| T-67 | Window 100 s, one 10 s outage entirely inside a 20 s excluded window → **denominator is 80**, SLA 100% | **must fail first** |
| T-68 | Same fixture → gross uptime is 90%, unchanged by exclusion | guard |
| T-69 | An outage partly inside and partly outside an excluded window is apportioned against the reduced denominator | **must fail first** |
| T-70 | A fully excluded window reports SLA as **not applicable**, not 100% | **must fail first** |
| T-71 | With no windows at all, gross and SLA are identical | guard |
| T-72 | The SLA CSV header column 9 reads `excluded_seconds` | **must fail first** |

**T-67 must assert the denominator itself, not just the percentage.**
With this fixture the ratio is 100% under *both* the old and new
arithmetic — a test checking only the percentage passes against the
defect. T-69 uses a partial overlap, where the two formulas visibly
diverge.

Record the computed denominator alongside the ratio, at baseline and
after. Those numbers show the formula changed rather than the inputs.

## Done

- All six tests pass; four baseline failures captured
- `docs/src/external-design.md` §8.1 records the column rename
- `docs/src/requirements.md`: FR-SLA-05, FR-SLA-09 → `Implemented`, G-12 struck
