# Handoff

One document per **subject**. Each is a self-contained unit of work: the
defect, what to build, what not to do, how to verify it, and what "done"
means. Hand a developer one subject and they can complete it without
reading anything else in this directory.

Each subject carries both a **Build** section for the implementer and a
**Verify** section for the tester, because the two are only meaningful
together — a test written without the reasoning behind the fix tends to
assert the implementation rather than the requirement.

## Subjects, in order

Work them in numeric order. Dependencies are stated in each header;
where two are independent, the header says so.

### M0 — provisionable (v0.28.0)

| # | Subject | Closes |
|---|---|---|
| 00 | [Toolchain pin and lint debt](00-toolchain-hygiene.md) | — prerequisite |
| 01 | [Migration set applies to an empty database](01-migration-applicability.md) | G-01 |
| 02 | [Retention deletes only what it archived](02-retention-scope.md) | G-20 |
| 03 | [Configuration defaults to production](03-configuration-templates.md) | G-21 |
| 03a | [Release archive unpacks flat](03a-release-archive-layout.md) | G-24 (archive half) |
| 03b | [The CI dependency scan actually runs](03b-ci-dependency-scan.md) | G-32 |
| 03c | [The format, lint and check gates actually run](03c-ci-toolchain-install.md) | G-33 |
| 03d | [The release archive is the tagged commit](03d-release-archive-source.md) | G-34 |

**0.28.0 shipped after 03c**, as a tag-only release and permanently
archive-less: it is tagged at a commit predating 03d, so the release
workflow there would invoke the old `package.sh`, and this project
supersedes releases rather than re-tagging.

**03d ships as its own patch release, 0.28.1** (owner's call,
2026-07-30) — the first release able to produce a distributable archive.
M1 renumbers to **0.28.2**.

### Producing a release

Follow [`RELEASE.md`](RELEASE.md) — a standing procedure, not a subject,
since release-candidate production recurs at every milestone. It carries
the fixed ordering (`package.sh` refuses when `HEAD` is not at the tag
matching the manifest version, so the bump precedes the tag) and the
asset-verification step.

**0.28.1 is the first release to use it.**

### M1 — audit trail trustworthy (v0.28.2)

| # | Subject | Closes |
|---|---|---|
| 04 | [Audit rows never deleted by retention](04-audit-retention-exemption.md) | G-04 |
| 05 | [Writer and verifier agree on chain order](05-audit-chain-ordering.md) | G-30 |
| 06 | [System-actor audit rows can be written](06-audit-actor-snapshot.md) | G-03 |
| 07 | [Audit write failures are surfaced](07-audit-write-surfacing.md) | G-26 |

### M2 — conformant and deployable (v0.29.0)

| # | Subject | Closes |
|---|---|---|
| 08 | [Import provenance and owner references](08-import-provenance-and-references.md) | G-05, G-31 |
| 09 | [Import replaces configuration, not history](09-import-replace-semantics.md) | G-22 |
| 10 | [Target state row and threshold location](10-target-state-and-thresholds.md) | G-06 |
| 11 | [Suppression windows honour their own flags](11-suppression-flags.md) | G-07 |
| 12 | [Suppression scope is exact and unambiguous](12-suppression-scope-and-tags.md) | G-08, G-09, G-27 |
| 13 | [SLA excludes suppressed time from the denominator](13-sla-denominator.md) | G-12 |
| 14 | [Automatic resolution records a duration](14-incident-duration-and-mttr.md) | G-10 |
| 15 | [One open incident per target, enforced](15-one-open-incident-per-target.md) | G-11 |
| 16 | [Incident actor columns carry one meaning each](16-incident-actor-columns.md) | G-29 |
| 17 | [Unreachable states are not representable](17-unreachable-states.md) | G-17, G-28 |
| 18 | [Schema constraints, timestamps and indexes](18-schema-integrity.md) | G-13, G-14, G-15 |
| 19 | [Identity keys on the OIDC subject claim](19-identity-subject-claim.md) | G-16 |
| 20 | [Per-endpoint OIDC overrides](20-oidc-endpoint-overrides.md) | G-19 |

**Subjects 08, 09 and 10 land in one branch.** Fixing 08 alone converts a
loud, safe failure into silent destruction of monitoring history.

### M3 — design frozen (v0.30.0)

| # | Subject | |
|---|---|---|
| 21 | [Screen re-expression spike](21-screen-re-expression-spike.md) | do first |
| 22 | [Multilingual mechanism](22-i18n-mechanism.md) | RFC 0009 |
| 23 | [Reduce the mockup to the decided scope](23-mockup-scope-reduction.md) | DEC-015 |
| 24 | [Type-aware target creation form](24-type-aware-target-form.md) | |

### M4 — interface integrated (v0.40.0)

| # | Subject | |
|---|---|---|
| 25 | [Design tokens and component layer](25-design-tokens-and-components.md) | gates 26-28 |
| 26 | [Re-express the thirteen existing screens](26-screen-re-expression.md) | one branch per screen |
| 27 | [Three new screens](27-new-screens.md) | may interleave with 26 |
| 28 | [Accessibility pass across the surface](28-accessibility-pass.md) | after 26, 27 |

### M5 — service complete (v1.0.0)

| # | Subject | Closes |
|---|---|---|
| 29 | [Notification delivery records](29-notification-delivery-records.md) | G-18 |
| 30 | [Turnstile activation](30-turnstile-activation.md) | RFC 0003 |
| 31 | [Failed-login audit recording](31-failed-login-audit.md) | RFC 0004 |
| 32 | [Slack payload enrichment](32-slack-enrichment.md) | RFC 0006 |
| 33 | [Tests move to sibling modules](33-test-module-migration.md) | G-23 |
| 34 | [Documentation language](34-packaging-and-language.md) | G-24 (language half) |
| 35 | [Cross-references resolve](35-cross-reference-integrity.md) | G-25 |
| 36 | [Release rehearsal and v1.0.0](36-release-rehearsal.md) | D-5 |

Subjects 33 and 34 depend on nothing and may run in parallel at any time.

## How to work a subject

1. **Tester first.** Write the tests marked *must fail first* against the
   current commit and capture their failure into `.git-exclude/evidence/`.
   NFR-QA-09 requires each fix to acquire a test that fails against the
   pre-fix behaviour, and that evidence is only obtainable before the fix
   lands.
2. **Implementer builds**, following the Build section.
3. **Tester confirms** the same tests now pass, plus the guards.
4. Update `docs/src/requirements.md` — status changed, gap **struck, not
   deleted**. Update `CHANGELOG.md`.
5. Capture gate output into `.git-exclude/evidence/` — see
   `.git-exclude/evidence/README.md`.

## Test numbering

Numbers are assigned once and **never reused or renumbered** — the same
discipline as RFC numbers (PRQ-03) and migrations (DR-MIG-02), and for
the same reason: a number that has appeared in captured evidence must
keep meaning what it meant.

A test added after a subject's register was written takes the **preceding
number with a letter suffix** — `T-01a`, `T-29a` — never the next free
number, which belongs to a later subject.

## Required review-request format

Governance requires a Handoff to state the review-request format, so it
is stated once here rather than repeated in every subject.

When a subject's build and tests are done, submit
`.git-exclude/review-request/NNN-slug.md` containing:

1. **Implementation summary** — what was built, in a paragraph
2. **Addressed requirements** — requirement IDs and gap IDs closed
3. **Changed files** — complete list, grouped code / tests / docs
4. **Important implementation decisions** — anything a reviewer would
   otherwise have to reverse-engineer, especially choices the subject
   left open
5. **Differences from the approved design** — anything you did that the
   subject did not literally specify, flagged as a judgment call rather
   than presented as specified
6. **Executed tests** — by number, with must-fail-first marked
7. **Test results** — counts, and the baseline-vs-now comparison
8. **Build and static-analysis results** — fmt, clippy, check, both WASM
   targets, migration gate, `cargo audit`
9. **Unresolved issues** — including anything you could not verify in
   this environment
10. **Known limitations** — disclosed, not omitted
11. **Requested review focus** — where you most want independent eyes

**Do not self-certify a subject as done.** That is the reviewer's call.
Items 5, 9 and 10 are the ones that have repeatedly turned out to matter
most — a disclosed judgment call is cheap to audit; an undisclosed one is
found late or not at all.

## Commit and tag conventions

- **Commit messages carry no `Co-Authored-By` trailer**, and no
  tool-attribution trailer of any kind. Project owner's rule.
- **Tags are bare versions** — `0.28.0`, not `v0.28.0`. Existing tags are
  `0.0.1`, `0.1.0`, `0.27.2`. The RFC `Status` field follows the tag
  form: `Implemented (0.29.0)`, which subject 35's T-161 checks
  mechanically. Note that RFC 000's own template illustrates
  `Implemented (v1.4.0)` — that is the generic policy's example, not
  Noye's form.
- **Three forms coexist deliberately. Do not "harmonise" them:**

  | Context | Form | Example |
  |---|---|---|
  | Git tag, RFC `Status` | **bare** | `0.28.0` |
  | Archive filename, from `package.sh` | **`v`-prefixed** | `noye-project-v0.28.0.tar.gz` |
  | Prose naming a release | **`v`-prefixed** | "Cut v0.28.0 (M0) after subjects 01–03 are merged" |

  The prose form was reviewed and **ratified by the project owner on
  2026-07-29**. Subject 35's cross-reference sweep must not strip it: a
  blanket removal would also break the archive filename, which is
  generated and correct.
- Tagging is the project owner's action, never the implementer's or the
  reviewer's.

## Standing rules

1. **Every closed gap acquires a regression test that fails against the
   pre-fix commit** (NFR-QA-09). A merge condition, not an aspiration.
   Name it in the pull request.
2. **Tests live in `src/<mod>/tests.rs`**, never inline. 40 files
   currently violate this (PRQ-05); subject 33 fixes them. Do not add a
   41st.
3. **Gate output is captured, not summarised.**
4. **An interface changes in `docs/src/external-design.md` before it
   changes in code**, per that document's §14.
5. **Hygiene that blocks verifying a subject gets its own pull request,
   merged first** — never bundled into a subject's PR. A PR nominally
   about one gap that also touches fifteen unrelated files cannot be
   reviewed for either purpose. If the hygiene PR would be large, stop
   and report rather than absorbing it.
6. **Stop and report beats improvising.** Where a subject marks a stop
   condition, the correct response to a failed assumption is *different
   work*, not a workaround.

## Where authority lives

A subject never overrides a specification. If they disagree, the
specification wins and the subject is wrong — report it.

| Question | Read |
|---|---|
| What must the system do? What is the acceptance criterion? | [`docs/src/requirements.md`](../../docs/src/requirements.md) |
| What does an outside observer see? | [`docs/src/external-design.md`](../../docs/src/external-design.md) |
| Why was something decided, and when do we revisit it? | [`docs/src/decision-log.md`](../../docs/src/decision-log.md) |
| What is deferred, and why? | [`ROADMAP.md`](../../ROADMAP.md), [`rfcs/`](../) |
| What is known-broken today? | `docs/src/requirements.md` §11 |

## Migration numbering

`0002` is retired and its number never reused (DEC-010).

| Migration | Subject | Adds |
|---|---|---|
| `0003` | 04 | Audit retention exemption |
| `0004` | 06 | Audit actor snapshot — rebuilds the hash-chained table |
| `0005` | 10 | Thresholds onto `targets` |
| `0006` | 11, 12 | SLA flag, tag relation, scope exclusivity |
| `0007` | 17, 18 | Constraints, timestamp normalisation, indexes |

## Escalation

| Situation | Goes to |
|---|---|
| An acceptance criterion cannot be turned into a test | Requirements architect — it is defective, and this is the most valuable signal a tester produces |
| You need an interface not in `external-design.md` | Function designer. Do not add it and document afterwards |
| You believe a requirement is wrong | Requirements architect, with the reasoning |
| A closed decision looks like it needs reopening | Maintainer, via an RFC |
| A stop-and-report condition triggers | Requirements architect |
