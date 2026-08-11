# 07f — A D1-backed behaviour gate in CI

**Milestone** M1.1 (infrastructure) · **Closes** no gap · **Wanted before** M2b
**Branch** `fix/07f-d1-ci-harness` · **Depends on** nothing
**Governing artifact** — `.git-exclude/reviewed/054-d1-ci-harness-proposal.md`,
Option **B** (owner's decision, 2026-08-11)

## Why this exists

**Four subjects have built a `wrangler dev --local` harness and thrown it
away** — 07a, 07d, 07e, and 08–10. Between them they found G-36, produced
`docs/src/d1-type-boundary.md`, confirmed G-42 on workerd, and verified
every behavioural claim in M2a.

**None of it runs in CI.** M2a's central claims — a re-import no longer
cascades, an unresolvable owner is refused before any write — live in
evidence logs. The source scans that *do* run guard the code's **shape**,
not its **behaviour**: they catch `INSERT OR REPLACE` returning, not a
cascade firing.

This project's four most severe defects were all found by executing code
that nothing executed. **This is the difference between that happening on
purpose and happening by luck.**

## Scope — Option B, and the boundary matters

**Drive the real HTTP routes and the real scheduled handler. Add no code
to the Worker.**

Option C — a feature-gated `/__test/` surface — was rejected: **a test
surface reaching a deployed Worker would be worse than G-21**, an
unauthenticated control surface rather than a credential to rotate, and
this project's history is that a thing which is safe because of a flag
nobody checks eventually ships.

**So the hard rule: this subject adds no route, no feature flag, and no
`#[cfg]` to `noye-core` or `noye-gateway`.** If a behaviour cannot be
reached through a route or the scheduled trigger, **it is out of scope —
escalate, do not reach for `/__test/`.**

`run_cleanup` is reachable via `wrangler dev --test-scheduled`, which
subject 07a's triage already confirmed exists. That is a production path,
not an invented one.

## ⛔ Step 0 — measure the cost before building anything

**Time a full local run end to end**: `worker-build --release`, `wrangler
dev --local` startup, migrations applied, one trivial assertion, teardown.

Report the number **before** writing the gate. The existing `wasm-tests`
job takes a few minutes; if this is dramatically worse, that changes
whether it belongs on every push or only on `main` and the weekly cron.

**If it exceeds roughly five minutes, stop and report** rather than
deciding the trigger policy yourself. A gate slow enough to be resented is
a gate someone eventually disables.

## Build

### 1. `scripts/check-d1-behaviour.sh`

Same shape as the four existing fixture gates: self-contained, its own
scratch state, teardown on exit via `trap`.

- Build Core, start `wrangler dev --local` against a **scratch D1 whose
  name is derived, not hardcoded** — `scripts/deployment-verify/`'s
  lesson, where a hardcoded `noye_db` was a blocking finding.
- Apply `sql/*.sql` in order.
- Seed fixtures, drive routes with the `X-Gateway-Token` / `X-Caller-*`
  headers the Gateway injects, assert on responses **and on the resulting
  database state**.
- Tear down. **Leave nothing under `crates/*/` — see T-209**; derive every
  path from a repo-root variable, never a relative `../../` after a `cd`.

### 2. The first assertions — M2a's, because they are the ones in logs

Start with what is currently unguarded, not with what is easy:

| | Behaviour | Currently guarded by |
|---|---|---|
| a | Re-importing an existing target **preserves** its check results, incidents and attachments (G-22) | an evidence log |
| b | An import naming an unresolvable `owner_id` is **refused before any write** (G-31) | an evidence log |
| c | An imported target **gets a `target_states` row** and is monitorable (G-06) | an evidence log |
| d | A retention pass completes and deletes only what it archived (DR-LIF-06) | an evidence log |

**(a) is the one that matters.** It is the assertion that would have
caught G-22 — silent destruction of monitoring history — and it cannot be
expressed as a source scan.

### 3. A CI job

Alongside the other gates. Trigger policy per Step 0's measurement.

### Do not

- **No `/__test/` route, no test feature, no `#[cfg]` in the Workers.**
  The rule this subject exists under.
- **Do not assert on log output.** Assert on responses and on database
  state. A log line is a description of behaviour; the row is the
  behaviour.
- **Do not port every evidence-log claim.** Four assertions that run
  beat twenty that time out. More can follow.
- **Do not touch real Cloudflare infrastructure.** Standing rule 7.

## Verify

| # | Test | Type |
|---|---|---|
| T-218 | The gate passes on `main` as it stands | guard |
| T-219 | **Each of (a)–(d) goes red when its fix is reverted** — proven per assertion, not assumed. This is the whole value of the subject | **must fail first** |
| T-220 | The gate leaves nothing behind: no `crates/*/.git-exclude`, no scratch `wrangler.toml`, no persist-to state — `git status` clean after a run **and after a failed run** | **guard — critical** |
| T-221 | `grep -rn "__test" crates/` finds nothing, and no new cargo feature exists — the Option C boundary held | **guard — critical** |
| T-222 | The gate fails loudly when `wrangler` or `workerd` is absent, rather than passing vacuously | **guard — critical** |

**T-219 is the subject.** A behaviour gate that has never been observed
failing is indistinguishable from one that asserts nothing — which is
G-32 and G-33 exactly.

**T-222 is the one most likely to be skipped.** A harness that silently
skips when its dependencies are missing is worse than no harness: it
reports green.

**T-220 covers the failed-run case deliberately.** Three of the four
throwaway harnesses left a stray `crates/core/.git-exclude/`, and the
fourth was invisible because `.wrangler/` is unanchored in `.gitignore`.

## Done

- Step 0's measurement reported and the trigger policy ruled on
- (a)–(d) asserted and each proven red
- The gate in CI, green on `main`
- `docs/src/development.md` gains it in the testing matrix, next to
  "Node is not Workers"
- **The evidence logs for (a)–(d) cite the gate** rather than standing
  alone — the point is that these stop being one-time observations

## Escalate

- **Step 0 exceeding ~5 minutes** → architect, before building.
- **A behaviour unreachable through a route or the scheduled trigger** →
  architect. That is the Option B/C boundary and it is not the dev team's
  to move.
- **Any assertion that cannot be made to go red** → architect. It is
  asserting nothing, and knowing which one matters more than having it.
