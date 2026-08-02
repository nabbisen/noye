# 07a — Drain the live-confirmation backlog off subject 36

**Milestone** M1 · **Closes** nothing on its own · **Unblocks** several residuals
**Branch** `fix/07a-live-residual-triage` · **Depends on** 04, 05, 07
**Governing artifact** — `.git-exclude/reviewed/030-subject-06a-access-boundary.md` §3

## Why this exists

Six obligations have accumulated on subject 36, the last subject in the
programme, each deferred there because it "needs live D1":

| # | Obligation | Origin |
|---|---|---|
| 1 | `RETENTION_BATCH_SIZE` — parameter ceiling and batches per invocation | DEC-017, subject 02 |
| 2 | DR-LIF-06 — archived set equals deleted set across batches | subject 02 |
| 3 | DR-LIF-07 — R2 fault injection; a failed archive deletes nothing | subject 02 |
| 4 | FR-AUD-06 — a full retention pass with an `audit_logs` policy row reinserted deletes zero audit rows | subject 04 |
| 5 | DEC-020 — `current_head_hash`'s per-write full-table walk cost | subject 05 |
| 6 | D-5 / provisioning from a clean Cloudflare account | subject 36 |

Two problems with leaving them there.

**They are unassignable.** Standing rule 7 forbids any agent from touching
real Cloudflare infrastructure. Subject 36 is written as a dev-team
handoff and every one of these needs infrastructure the dev team may not
touch. As it stands, nobody can execute it.

**They compound.** There were four when this was first noted
(`.git-exclude/reviewed/029-subject-06-escalations.md` §4); the architect
added two more within a week without noticing the pile. A subject that
absorbs deferrals is a subject that silently becomes the riskiest one in
the programme, and it is scheduled last — the worst possible place to
discover it cannot be done.

**This subject drains the pile as far as it can be drained without live
infrastructure**, and turns what remains into something the owner can run
in one prepared sitting.

## The distinction that governs everything here

`wrangler --local` is a **real D1 runtime**, not a SQLite stand-in. That
is not a guess: subject 06's Step 0 showed `PRAGMA foreign_keys = 1` and a
genuine foreign-key refusal under `--local`, where raw `sqlite3` — default
`foreign_keys = 0` — silently accepted the identical insert.

So local emulation is a **substantially better substrate than this project
has been treating it as**, and it is inside the access boundary.

It is still **not the deployment.** Anything confirmed here is
*"confirmed against the local D1 runtime"* and must be recorded in exactly
those words — never "confirmed live." This project has been bitten
repeatedly by a control that looked verified and was not; conflating these
two would be the same mistake in a new place.

## Build

### Step 1 — triage, and report before executing

For each of the six, determine **whether the local runtime can confirm
it**, and say why. Report that determination to the architect **before
executing anything.**

This ordering is deliberate. I do not know what Miniflare's R2 emulation
can do about fault injection, whether cron invocations can be driven
locally, or whether D1's bound-parameter ceiling is the same locally as
remotely — and **specifying mechanisms I have not verified is what caused
four defective Build steps in this milestone.** The triage is your
finding, not my assumption.

For each, report one of:

| Verdict | Meaning |
|---|---|
| `LOCAL` | The local runtime confirms it faithfully. Execute in step 2 |
| `LOCAL-PARTIAL` | Some property confirmable locally, some not. State the split precisely |
| `DEPLOYMENT` | Only a real deployment can answer it. Goes to step 3 |

**A number that is meaningless locally is `DEPLOYMENT`, not `LOCAL`.** #5
is the obvious case — a timing measurement on a laptop says nothing about
a Worker's CPU budget. Do not produce a number just because a command
runs.

### Step 2 — execute everything you marked `LOCAL`

Ordinary work: run it, capture the output verbatim into
`.git-exclude/evidence/`, and update `docs/src/requirements.md` for each
residual that closes — with the status wording *"confirmed against the
local D1 runtime,"* not *"confirmed."*

For anything that closes, the corresponding line in subject 36 comes out.
**Editing 36 is part of this subject**, not an afterthought: the point is
that the pile shrinks.

### Step 3 — turn what is left into one prepared sitting

For each `DEPLOYMENT` item, produce:

- **one runnable script** the owner executes without editing, and
- **a capture form** — exactly what output to paste back, so a result
  lands in evidence verbatim rather than as prose.

Package them so the whole remaining set is **one session**, not six
requests spread over months. That is the sustainability goal: the owner's
manual involvement should be bounded and prepared, never ad hoc.

**Do not run any of these yourself**, and do not ask the owner to run them
in this round — step 3's deliverable is the prepared package, and when it
is used is the owner's call.

### Step 4 — make the migration gate faithful to D1's atomicity

`scripts/check-migrations.sh` applies each file with bare `sqlite3` and
trusts the exit code. **Bare `sqlite3` does not stop at the first error**
— it prints the error, returns nonzero, and *keeps executing the rest of
the file*:

```
$ sqlite3 db < multi.sql          # statement 3 errors
naive exit=1
  statement 4's table:  created   ← it kept going
$ sqlite3 -bail db < multi.sql
-bail exit=1
  statement 4's table:  absent
```

There is **no live defect** — T-01 fails on the nonzero exit and aborts
before any later check reads the database. But the gate does not model
D1's all-or-nothing behaviour, and a migration whose second statement
fails would leave a partially-applied fixture the moment that ordering
changes.

Add `-bail`, and an explicit transaction wherever atomicity is the
property under test. Subject 06's T-29a already does this; make it the
gate's default rather than one test's local fix.

**Found by the dev team while writing T-29a** — where the naive
invocation reported failure while silently completing the `DROP`,
`RENAME` and `CREATE INDEX` anyway, which would have made T-29a pass on a
false premise. It is the second substrate trap in that subject alone, the
first being bare `sqlite3`'s `foreign_keys = 0`. Both have the same shape:
**a local stand-in reporting a plausible result while behaving unlike the
thing it stands in for.** That is this project's recurring defect, and
this step is where the general form of it gets closed.

### Do not

- **Do not touch real Cloudflare infrastructure.** Standing rule 7. The
  same rule that made 06a's original Build step 3 defective.
- **Do not record a local result as a live one.** The wording is the
  deliverable as much as the number.
- **Do not fix anything you find.** If a local run shows a real defect —
  entirely possible; four of these have never been executed anywhere —
  **stop and report**. A residual-confirmation subject that starts
  changing behaviour is two subjects wearing one branch.

## Verify

| # | Test | Type |
|---|---|---|
| T-36a | Every one of the six carries a `LOCAL` / `LOCAL-PARTIAL` / `DEPLOYMENT` verdict with a stated reason — none silently dropped | guard |
| T-36b | Each executed item's evidence names the substrate it ran on, in the requirement's own status line | **guard — critical** |
| T-36c | Subject 36 no longer lists anything closed here, and still lists everything not closed | guard |
| T-36d | Each `DEPLOYMENT` script runs end-to-end against **local** emulation first — so the owner is not the one who discovers it has a typo | guard |

**T-36d is the one that earns the owner's trust.** A script handed to a
human to run against real infrastructure, which then fails on a syntax
error, spends the scarcest resource this project has. Prove it runs
somewhere before it is handed over.

**T-36b is the one that protects the record.** The whole value of this
subject is an honest ledger of what has actually been observed and where.
A local result recorded as live would leave the project worse off than
leaving the residual open.

## Done

- All four tests pass
- Every `LOCAL` item executed, with evidence and an amended requirement
  status naming the substrate
- Subject 36 amended — shorter by exactly what closed
- The `DEPLOYMENT` package exists, is self-contained, and has been run
  against local emulation

## Escalate

- **Any local run revealing a real defect** → architect, before fixing.
- **A triage verdict you are unsure of** → report the uncertainty as the
  verdict. `LOCAL-PARTIAL` with a precise split beats a confident `LOCAL`
  that turns out to have measured the emulator.
- **If more than two of the six come back `DEPLOYMENT`** → architect. That
  would mean the deferral problem is structural rather than incidental,
  and the answer is a decision about the project's verification strategy,
  not more scripting.
