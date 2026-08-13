# 07g — Retention's trigger reads the wall clock, not the event

**Milestone** M1.1 (infrastructure) · **Closes** G-43 · **Unblocks** 07f's (d)
**Branch** `fix/07g-scheduled-event-time` · **Depends on** 07f
**Governing artifact** — Gap **G-43** (§11)

## The defect

`crates/core/src/monitor/engine.rs:87`:

```rust
// 6. Data lifecycle: periodic cleanup (runs at minute 0 of every hour)
if now.format("%M").to_string() == "00"
    && let Err(e) = db::retention::run_cleanup(env).await
```

`now` is `chrono::Utc::now()`, read at handler start. The cron is
`* * * * *` — **every minute**. And `crates/core/src/lib.rs:96`:

```rust
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
```

**The invocation's own scheduled time is received and discarded.**
`worker`'s `ScheduledEvent` exposes `schedule()` — the nominal time — and
`cron()`.

### Why this matters in production, not just in a test

Cloudflare's cron triggers are **best-effort, not exact.** An invocation
nominally for 00:00 that starts at 00:01 reads `"01"`, the condition is
false, and **retention does not run that hour.** Same for a retried
invocation.

**And nothing is logged either way** — the branch simply does not fire.
There is no error, no warning, and no way to tell from the outside
whether retention ran.

That is this project's signature defect for the fourth time — after G-21,
G-32 and G-33 — a control that appears configured, may never execute, and
is silent about it.

## Build

1. **Thread the event through.** `scheduled()` currently binds it as
   `_event`. Pass it, or the value derived from it, to
   `run_scheduled_checks`.
2. **Decide the hour from the event's nominal time**, not from
   `Utc::now()`. `schedule()` returns the scheduled time; derive the
   minute from that.
3. **Log when retention is skipped**, at least at debug level. A branch
   that silently does nothing is the half of this defect that made it
   invisible, and fixing the trigger without fixing the silence leaves
   the next such bug just as hard to see.

### Do not

- **Do not change what `run_cleanup` does.** This subject changes *when
  it is invoked* and nothing else.
- **Do not widen into the other five steps** of `run_scheduled_checks`.
  They use `now` for timestamps and eligibility, which is correct — a
  check result is stamped when it ran, not when it was scheduled.
  **Only the retention gate is wrong**, because it is the only place
  `now` is used to answer *"is this the invocation that should do X?"*
- **Do not make the interval configurable.** Hourly is DR-LIF-05's
  cadence and is not in question here.

## Verify

| # | Test | Type |
|---|---|---|
| T-223 | With a scheduled event whose nominal time is minute `00` but whose wall clock is not, retention **runs** — the production skip this closes | **must fail first** |
| T-224 | With a nominal time that is not minute `00`, retention does **not** run, and a skip is logged | guard |
| T-225 | The other five steps of `run_scheduled_checks` still use wall-clock `now` for timestamps — a check result is stamped when it ran | **guard — critical** |
| T-226 | **07f's (d)** — DR-LIF-06 asserted in `scripts/check-d1-behaviour.sh`: a retention pass deletes only what it archived, driven by a nominal-time event | **must fail first** |

**T-223 is the defect.** Everything else is guarding against fixing it
wrongly.

**T-225 is the one to be careful about.** The temptation is to replace
every `now` with the event's time. That would be a regression: a check
result stamped with its *scheduled* time rather than its *actual* time
would misreport when a probe happened, which matters for SLA and incident
duration.

**T-226 is why this subject exists now rather than later** — it is the
assertion 07f could not reach, and it lands in the gate 07f built.

## Done

- All four tests pass; T-223's and T-226's baselines captured
- `docs/src/requirements.md`: G-43 struck
- `CHANGELOG.md` — it publishes verbatim
- **07f's assertion set goes from three to four**

## Escalate

- **`schedule()` not returning what its name implies** under
  `wrangler dev --local` — check it against workerd before building on
  it, per `development.md` § "Node is not Workers". If it disagrees with
  the docs, stop and report.
- **Any of the other five steps turning out to need the nominal time** →
  architect. That would mean this defect is wider than the retention gate
  and the subject's scope is wrong.
