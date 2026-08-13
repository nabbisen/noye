# Scheduled-event nominal time — production confirmation

Subject 07g (G-43), item #7 of the deployment-verify package — added
2026-08-13 on the architect's ruling
(`.git-exclude/reviewed/058-subject-07g-escalation-ruling.md` §4).
Governing question: **does `event.schedule()` return the invocation's
*nominal* scheduled time on real Cloudflare infrastructure, the way
its name implies?**

This is not a script, because the thing under test *is* real Workers
behaviour — nothing local can answer it. Subject 07g fixed G-43
(retention's hourly trigger read the wall clock instead of the
invocation's nominal schedule, so a cron trigger arriving even
slightly late silently skipped the hour) by reading `event.schedule()`
instead. That the fix compiles and passes its host tests
(`retention_trigger`, T-223/T-224) is not in question. **That
`event.schedule()` carries the value its name implies, in production,
is the one inference this subject could not turn into an observation.**

## Why this couldn't be confirmed locally

`wrangler dev --local`'s `--test-scheduled` harness exposes
`/__scheduled?time=<ms>` to drive a scheduled event with a chosen
nominal time. Confirmed during 07g's escalation
(`.git-exclude/evidence/subject-07g-schedule-time-escalation.log`):
the parameter never reaches the compiled Worker. Traced one layer
further by the architect: Miniflare's own entry layer parses `time`
correctly and constructs `scheduledTime`; the `worker` crate
(`worker-0.8.5`) reads the right property off the event object it's
given. **The loss is in workerd's local-mode scheduled-event
simulation, below both of the layers this project's code touches** —
under `wrangler dev --local`, `event.schedule()` *is* the wall clock,
always, regardless of what's requested.

So the local `wrangler dev --local` instrument — this project's only
one, per standing rule 7 — cannot distinguish "the fix reads the right
value" from "the fix reads a value that happens to look right because
it's always just the current time." Only a real Cron Trigger firing
against real Workers settles it.

## What to do

You need a deployment with a Cron Trigger active (any interval is
fine — the point is observing what `event.schedule()` reports relative
to when the invocation actually ran, not exercising the retention
gate's minute-`00` logic specifically).

1. Deploy Core with its Cron Trigger configured as normal
   (`crates/core/wrangler.toml`'s `[triggers]` block).
2. Let at least a few real invocations fire. Cloudflare's dashboard
   (Workers → your Core service → Triggers → Cron Triggers → view
   logs) or `wrangler tail` shows each invocation's actual wall-clock
   time.
3. **It costs one log line.** Subject 07g's fix logs every non-firing
   invocation: `"Retention skipped this invocation: nominal schedule
   was minute {}, not 00"`. Read a handful of these against the
   invocation's real wall-clock time (from the dashboard or
   `wrangler tail`'s own timestamp):
   - If the logged minute **tracks the schedule** — i.e., stays
     whatever the Cron Trigger's configured minute pattern implies,
     even when the dashboard shows the invocation actually landed a
     few seconds or more into the next minute — `event.schedule()` is
     the nominal time, as documented, and this item closes clean.
   - If the logged minute **tracks the wall clock instead** — moves
     with `wrangler tail`'s own timestamp rather than staying fixed at
     the scheduled minute — then `event.schedule()` does not do what
     Cloudflare's own docs say even in production, which would be a
     new and more serious finding than G-43 itself (report it, don't
     silently work around it).
4. If your Cron Trigger fires every minute (as Core's default
   `* * * * *` does), you'll see both outcomes naturally: most
   invocations skip and log the minute; the one that lands on `:00`
   runs retention silently (as designed — success stays quiet, only
   the skip is logged). Confirm you see at least one of each.

## Capture form

Paste into the evidence log verbatim:

```
Deployment: <account / zone, scratch or existing>
Date: <date>
Cron pattern: <crates/core/wrangler.toml [triggers] value at time of test>
Wrangler version: <wrangler --version>

Invocations observed (from the dashboard or `wrangler tail`), each as:
  - Actual wall-clock time of invocation:
  - Logged skip line (verbatim), or "ran silently" if none:
  - Nominal minute implied by the cron pattern at that invocation:
  - Logged minute matches the nominal minute (not the wall-clock minute)? Y/N

At least one "ran silently" (minute 00) and at least one skip observed? Y/N

Overall: does `event.schedule()` return the nominal scheduled time on
real Cloudflare infrastructure? <yes / no, with the deciding reason>
```

## When you're done

If confirmed, this closes the last open question from G-43/subject
07g — nothing further to fix, just record the observation. If it
disagrees with Cloudflare's documented behaviour, that is a new
finding: report it before assuming the fix is safe in production, the
same "stop and report" discipline 07g itself used when the local
instrument disagreed with the docs.

Fold the result into whichever evidence log records this
deployment-verify session (see the package `README.md`'s "Capture
form" section) rather than a separate file — this is one more line in
the same dated record as items 1, 5 and 6.
