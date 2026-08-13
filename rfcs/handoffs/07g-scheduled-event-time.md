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

4. **Extract the decision into a pure function** — added 2026-08-13 after
   the escalation. Everything the gate decides is a function of one
   `f64`:

   ```rust
   enum RetentionTrigger { Run, Skip { minute: String }, UnreadableSchedule }
   fn retention_trigger(scheduled_ms: f64) -> RetentionTrigger
   ```

   `run_scheduled_checks` calls it and acts on the result.

   > **Why:** `wrangler dev --local`'s `/__scheduled?time=` does not reach
   > `event.schedule()` — workerd's local scheduled-event construction
   > substitutes the current instant. Miniflare passes the parameter
   > correctly and the `worker` crate reads the right property; the loss is
   > below both (`.git-exclude/reviewed/058-…` §1). So the nominal time
   > cannot be controlled locally, and a test that needs to control it
   > cannot be written.
   >
   > Separating the decision from the runtime that supplies its input is
   > what this project already does for `decide_transition`, `walk_chain`,
   > `compute_cutoff` and `bool_from_d1`. **What stays unverifiable
   > locally then shrinks to one line** — that `event.schedule()` carries
   > the nominal time — which is Cloudflare's documented API and is routed
   > to the deployment session.

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
| T-223 | `retention_trigger(<a timestamp at minute 00>)` yields `Run` — a **host** test; no Worker runtime, no controllable nominal time | **must fail first** |
| T-224 | `retention_trigger(<minute 37>)` yields `Skip { minute: "37" }`, and an unrepresentable value yields `UnreadableSchedule` | guard |
| T-225 | The other five steps of `run_scheduled_checks` still use wall-clock `now` for timestamps — a check result is stamped when it ran | **guard — critical** |
| ~~T-226~~ | ~~07f's (d), DR-LIF-06 driven by a nominal-time event~~ — **struck 2026-08-13.** The fix does not make retention drivable on demand locally, because under workerd's local mode `schedule()` *is* the wall clock. **07f stays at three assertions**, recorded rather than papered over: an assertion that appears to test DR-LIF-06 but cannot trigger the pass is worse than a known gap |
| T-227 | In `scripts/check-d1-behaviour.sh`: a scheduled tick at **any** minute produces exactly one observable outcome — retention ran, or the skip line appeared naming the minute. Deterministic whatever the clock says, and it asserts the half of this subject that is about the **silence** rather than the timing | **must fail first** |

**T-223 is the defect.** Everything else is guarding against fixing it
wrongly.

**T-225 is the one to be careful about.** The temptation is to replace
every `now` with the event's time. That would be a regression: a check
result stamped with its *scheduled* time rather than its *actual* time
would misreport when a probe happened, which matters for SLA and incident
duration.

**T-227 is what this subject can prove in CI.** The timing half is now a
host test (T-223/T-224); the silence half — that a skipped retention says
so — is observable through the real trigger regardless of what minute it
is.

## Done

- All four tests pass; T-223's and T-226's baselines captured
- `docs/src/requirements.md`: G-43 struck
- `CHANGELOG.md` — it publishes verbatim
- **07f's assertion set stays at three**, plus T-227 — the reason struck into the table above, not left implicit

## Escalate

- **`schedule()` not returning what its name implies** under
  `wrangler dev --local` — check it against workerd before building on
  it, per `development.md` § "Node is not Workers". If it disagrees with
  the docs, stop and report.
- **Any of the other five steps turning out to need the nominal time** →
  architect. That would mean this defect is wider than the retention gate
  and the subject's scope is wrong.
