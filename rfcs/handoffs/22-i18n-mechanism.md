# 22 — Multilingual mechanism

**Milestone** M3 · **Satisfies** NFR-I18N-02, NFR-I18N-03
**Implements** [RFC 0009](../proposed/009-multilingual-interface.md) (accepted, DEC-016)
**Branch** `feat/22-i18n` · **Depends on** subject 21
**Governing artifact** — **RFC 0009** (accepted, DEC-016)

## Why now

NFR-I18N-01 has been `Not met` since the baseline, with no tracking
artifact — the state the RFC lifecycle policy calls silent withdrawal.
DEC-016 resolved it: the requirement stands, English and Japanese.

**Sequencing is the point.** String externalisation is far cheaper before
the interface rebuild than after. Building the mechanism now means each
screen converts as it is re-expressed in M4 — one pass instead of two.

## Build

1. **String table.** Static, keyed by enum, resolved at render time. No
   runtime file loading and no new dependency: the UI layer is pure
   functions returning `String` and the table must be too, so it stays
   host-testable (NFR-QA-01, NFR-QA-02).
2. **Selection and persistence.** A cookie, working without JavaScript
   (NFR-A11Y-10) — a form post or a link, never a client-side switch.
   Share the preference channel RFC 0001 sketches for the theme toggle
   rather than inventing a second one. An unrecognised value falls back
   to the default, exactly as `?tab=` and `?window=` already do.
3. **`<html lang>`** reflects the active language. It is hard-coded to
   `en` at `crates/gateway/src/ui/layout.rs:197`. **A wrong `lang` is
   worse than none** — screen readers use it to pick a voice.
4. **Convert the shell only**: navigation, page header, user chip,
   footer, skip link. Not the thirteen screens — those convert as they
   are re-expressed in subject 26.

### Do not translate

Notification message bodies, CSV headers, configuration keys, log
output, audit `action_type` values. All are machine-facing contracts and
translating one breaks integrations. This is the most likely
well-intentioned mistake in this subject.

## Verify

| # | Test | Type |
|---|---|---|
| T-103 | No user-visible literal remains in the shell's rendering functions | **must fail first** |
| T-104 | Language selection persists across a reload | **must fail first** |
| T-105 | …and works with scripting disabled | **must fail first** |
| T-106 | An unrecognised language cookie falls back to the default rather than erroring | **must fail first** |
| T-107 | `<html lang>` matches the active language | **must fail first** |
| T-108 | Contrast pinning passes in both languages | **guard — critical** |
| T-109 | Notification bodies, CSV headers, config keys and audit action types are **not** translated | **guard — critical** |

No test may assert on translated display text. Assert on structure or on
the string-table key.

## Done

- All seven tests pass; five baseline failures captured
- `docs/src/requirements.md`: NFR-I18N-02, 03 → `Implemented`;
  NFR-I18N-01 → `Partial` until subject 26 completes the screens

## Escalate

A pinned contrast pair failing in Japanese → change the design, never the
pinned value. Report it.
