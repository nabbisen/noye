# Deployment verification package (subject 07a, Step 3)

The `DEPLOYMENT`-only remainder of subject 07a's original six-item
backlog (`rfcs/handoffs/07a-live-residual-triage.md`) — the three
things `wrangler dev --local` cannot answer, because they're properties
of Cloudflare's real platform (a subrequest/CPU-time budget, real
network latency to D1, a real Cloudflare account's UI), not of this
project's own logic:

| # | What | Script |
|---|---|---|
| 1 | DEC-017's subrequest-budget half — batches completed per scheduled invocation before the platform cuts it off | `01-retention-batches-per-invocation.sh` |
| 5 | DEC-020 — `current_head_hash`'s per-write cost at a realistic `audit_logs` size | `02-audit-chain-write-cost.sh` |
| 6 | Is `docs/src/setup.md` sufficient for a first-time deployer against a real account | `03-onboarding-checklist.md` |
| 7 | **Does `event.schedule()` return the invocation's *nominal* time on real Workers?** | `04-scheduled-event-time.md` |

**This is one prepared sitting, not three separate requests.** Run
them together, in one session, in the order above — #6 naturally
happens first in practice (you need a deployed instance before you can
seed data against it), but #1 and #5 don't depend on each other and
can run in either order once you have one.

## Item 7 — added 2026-08-13, and it is one log line

Subject 07g fixed **G-43**: retention's hourly trigger read the wall clock
instead of the invocation's nominal schedule, so a cron trigger arriving
even slightly late silently skipped the hour.

The fix reads `event.schedule()`. **That it returns the nominal time
cannot be confirmed locally** — `wrangler dev --local`'s
`/__scheduled?time=` never reaches it, because workerd's local
scheduled-event construction substitutes the current instant. Miniflare
passes the parameter correctly and the `worker` crate reads the right
property; the loss is below both.

So this is the last inference in that subject, and a real deployment is
the only place it becomes an observation. **It costs one log line**: the
skip message names the minute it saw. On a deployment whose cron fires
every minute, watch whether the minute it reports tracks the schedule or
the clock.

## Before you run anything against `--remote`

Every script here was proven end-to-end against `wrangler dev --local`
first (T-187 — "the owner is not the one who discovers it has a
typo"). That local run is *only* a correctness check on the script's
SQL and control flow; the **numbers** it produces locally are not the
deliverable and must not be recorded as the answer to DEC-017 or
DEC-020 — both explicitly require a real deployment (see each script's
header comment for why). Local proof evidence:
`.git-exclude/evidence/subject-07a-step3-local-proof.log`.

**Nobody but you should run the `--remote` invocations.** Per the
subject's own rule, the dev team does not touch real Cloudflare
infrastructure — these scripts exist so that when you're ready, it's a
single prepared session rather than something you have to design from
scratch.

**Run this against a scratch deployment, not `noye_db`.** Scripts `01`
and `02` take the D1 database name as a required argument with no
default — deliberately, because `02 --remote <db> seed <user>
50000` injects fifty thousand fabricated rows into `audit_logs`, and
running that against a real production database pollutes a
tamper-evident trail permanently (`audit_logs` never expires),
degrades every subsequent audit write until `cleanup` runs (the exact
cost DEC-020 exists to measure), and makes your own integrity check
report the rows as `orphaned`. **`03-onboarding-checklist.md`
provisions exactly the right substrate for this**: a clean account you
stand up from scratch, where seeding tens of thousands of synthetic
rows costs nothing but time. Point `01` and `02` at that deployment's
database name, not an existing one anything else depends on.

### The scripts now enforce that, rather than only documenting it

Added 2026-08-16, during release-readiness preparation. The paragraph
above was the only thing standing between a mistyped argument and fifty
thousand fabricated rows in a real `audit_logs` — and it is a paragraph
you would be reading *before* a session, not at the moment you type the
command.

Two guards, in both `01` and `02`:

1. **`noye_db` (and `noye-db`) is refused outright**, in `--local` and
   `--remote` alike. The name is the concern, not the mode.
2. **Before anything destructive on `--remote`** — `seed` or `cleanup` —
   the script prints what it is about to do, to which database, and
   makes you **re-type the database name**. A mismatch aborts before any
   `wrangler` invocation. Read-only subcommands (`check`, `count`) are
   unaffected, so the measurement loop stays friction-free.

Both guards were tested on their refusal paths only — no path that could
reach `wrangler --remote` was executed.

```
$ 01 --remote noye_db seed t1
ERROR: refusing to run against 'noye_db': this script seeds and deletes
rows and is for a SCRATCH database only. …

$ 01 --remote scratch-verify-db seed t1
About to seed check_results rows in REMOTE D1 database "scratch-verify-db".
Re-type the database name to confirm: _
```

## What each script needs from you

- **01** and **02** both need the D1 database name as their second
  argument (after `--local`/`--remote`) — see above.
- **01** needs the id of an existing target in your `targets` table.
- **02** needs the id of an existing user in your `users` table, and a
  target total row count for `audit_logs` (it tops up from whatever's
  there now, at each checkpoint you choose — e.g. 1,000, then 10,000,
  then 50,000).
- **03** needs a genuinely clean Cloudflare account (or an explicit
  note about what wasn't clean) and time to walk through
  `docs/src/setup.md` start to finish.

Every script supports `--local` or `--remote` as its first argument —
there is no default, so you cannot run one against real infrastructure
by accident. The database name has no default either, for the same
reason.

## Cleanup

`01` and `02` both tag every row they seed with a distinctive `id`
prefix (`verify-batch-`, `verify-audit-cost-`) and both provide a
`cleanup` subcommand that deletes exactly those rows and nothing else.
`01`'s seeded rows are also self-cleaning in the ordinary case —
that's the measurement, retention deletes them — `cleanup` is only for
aborting a run partway through. `02`'s seeded `audit_logs` rows do
**not** self-clean (that table is non-expiring by design, G-04) — run
`02 ... cleanup` when you're done measuring, or they sit there
permanently.

## Capture form

Each script prints its own capture form at the end of the step that
produces a number. Paste all three (plus 03's) into one evidence log —
suggested path: `.git-exclude/evidence/subject-07a-step3-deployment.log`
— so DEC-017, DEC-020 and the onboarding finding land as one dated
record, not three.

## When you're done

Update `docs/src/decision-log.md` (DEC-017 with the measured batches-
per-invocation number, DEC-020 with the measured write-cost numbers)
and `rfcs/handoffs/36-release-rehearsal.md` (these three items come out
of it, the same way subject 07a's local-confirmable items already
did). That edit is yours or the architect's to make from the captured
numbers — not something to script in advance of having a real number
to write down.
