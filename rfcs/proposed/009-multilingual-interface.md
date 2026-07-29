# RFC 0009: Multilingual interface

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-07-28
**Related ROADMAP item**: Phase 5 — design freeze; resolves open decision D-3
**Estimated size**: medium
**Implementation target**: 0.30.0 (Phase 5) if accepted; withdrawal otherwise

---

## Summary

The project's development instructions state that the GUI must support
multiple languages. Nothing implements it, and no artifact tracks the
work. This RFC exists so the requirement is either **owned or formally
withdrawn** — the one thing it must not remain is stated-but-unowned,
which the RFC lifecycle policy names as silent withdrawal.

## Background

`docs/src/requirements.md` §7.6 carries NFR-I18N-01 through 05.
NFR-I18N-01 is marked **Not met**: the shipped interface is English
only. `crates/gateway/src/ui/layout.rs` hard-codes `<html lang="en">`,
there is no string table, and user-visible text is embedded directly in
the rendering functions.

Open decision D-3 has carried since the baseline.

Two facts bear on the answer:

- **The UI mockup already ships EN + JA.** `noye-mockup` v0.6.10 has an
  `i18n.rs` with a static translation table covering navigation, roles
  and core chrome, and its own handoff records i18n coverage as partial
  (its RISK-001). So the question is not "is this feasible" — it is
  "is this wanted in the product".
- **The cost is asymmetric in time.** String externalisation is
  substantially cheaper before Phase 6 re-expresses the interface than
  after. Deciding late means touching every screen twice.

## The decision this RFC forces

**Accept** — commit to it, and schedule externalisation into Phase 5 so
Phase 6 builds on it.

**Withdraw** — mark NFR-I18N-01…04 `Withdrawn` in `requirements.md`
(marked, never deleted, per §15), record the reason in the decision log
with re-evaluation criteria, and amend the development instructions so
they stop asserting a requirement the project has declined.

There is no third option in which the requirement stands and nothing
happens. That is the current state and it is the failure mode.

## Design, if accepted

### Scope

Interface strings only. Explicitly **not** in scope: notification
message bodies, CSV headers, configuration keys, log output, or audit
`action_type` values — all of those are machine-facing contracts, and
translating them would break integrations.

### Mechanism

A static string table keyed by an enum, resolved at render time. No
runtime file loading, no external dependency: the UI layer is pure
functions returning `String`, and the table should be too, so it stays
testable on the host target (NFR-QA-01, NFR-QA-02).

### Selection and persistence

NFR-I18N-03 requires selection to be explicit and to persist. A cookie
matching the pattern already sketched for the theme toggle in RFC 0001
is the obvious mechanism, and the two should share it rather than
inventing two preference channels.

Selection must work without JavaScript (NFR-A11Y-10), so it is a form
post or a link, not a client-side switch.

`<html lang="…">` must reflect the active language. It is currently
hard-coded, and a wrong `lang` is worse than none — screen readers use
it to choose a voice.

### Interaction with the accessibility guarantees

This is the part that needs care, and it is why the RFC is medium rather
than small.

`crates/gateway/src/ui/layout/contrast.rs` pins 25 colour pairs. That
part is language-independent. But **layout is not**: German compounds
and Japanese line-breaking behave differently from English, and a
navigation label that fits at one width may not at another. NFR-I18N-04
requires that locale handling not compromise §7.1, so:

- Nothing may rely on a label's rendered width.
- Truncation must not be the mechanism that makes a layout fit.
- Tests asserting on English literals need to assert on structure or on
  the string-table key, not on the visible text.

The last point is the real cost. It is also a latent improvement: a test
that asserts `"All clear — no open incidents right now."` is testing the
copy, not the behaviour.

### Languages at launch

English and Japanese, matching the mockup. Adding a third is then a
table entry, not a design change.

## Requirements

If accepted, NFR-I18N-01, 02 and 03 move from `Not met` to `Deferred`
with this RFC as their tracking artifact, and to `Implemented` on
delivery. NFR-I18N-04 becomes applicable.

If withdrawn, all four are marked `Withdrawn` with a dated reason.

## Test plan

- Every user-visible string resolves through the table; no literal
  remains in a rendering function.
- Selecting a language persists across a reload.
- Selection works with scripting disabled.
- `<html lang>` matches the active language.
- Contrast pinning passes in both locales.
- Landmarks, skip link and `aria-current` are present in both locales.
- No test asserts on translated display text.

## Security considerations

The string table is static and compiled in, so there is no
locale-driven input path and no format-string injection surface. The
selection cookie carries a value from a closed set and must be validated
against it rather than reflected — an unrecognised value falls back to
the default, consistent with how `?tab=` and `?window=` already behave.

## Out of scope

- Right-to-left layout. It is a real body of work and no target
  language needs it; adding Arabic or Hebrew later would be its own RFC.
- Locale-dependent number and date formatting. Timestamps are already
  machine-readable with an explicit zone (NFR-I18N-05, met).
- Translating the documentation tree. Separate concern, separate
  decision.
