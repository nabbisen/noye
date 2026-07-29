# 24 — Type-aware target creation form

**Milestone** M3 · **Satisfies** FR-TGT-03, FR-TGT-09
**Branch** `feat/24-target-form` · **Depends on** subject 23
**Governing artifact** — Requirement **FR-TGT-09** · mockup RISK-002

## Why this exists

Closes the mockup's own RISK-002: specified in its handoff Appendix F,
never implemented.

Today the create form asks an operator to ignore fields that do not apply
to their chosen protocol. HTTP needs expected status and body substring;
TCP needs neither; TLS needs a threshold in days. **This is the one
screen where the current interface is actively confusing**, rather than
merely plain.

## Build

Choose a protocol, then show only the fields that protocol uses.

Re-render server-side on `?type=` — no client-side field toggling
(NFR-A11Y-10). This follows the pattern already used for `?tab=` and
`?window=`: view state in the URL, linkable and reload-safe.

Summarise the failure conditions for the chosen type in plain language,
as the target detail screen already does on its Overview tab.

Per-type validation must reject a value the protocol cannot use — an
expected status code on a TCP target is not a warning, it is a rejection.

## Verify

| # | Test | Type |
|---|---|---|
| T-113 | Each protocol shows only its own decision criteria | **must fail first** |
| T-114 | The form re-renders server-side on `?type=`, with scripting disabled | **must fail first** |
| T-115 | Per-type validation rejects a value the protocol cannot use | **must fail first** |
| T-116 | An unrecognised `?type=` falls back rather than erroring | guard |

## Done

- All four tests pass; three baseline failures captured
- `docs/src/external-design.md` §4.4 records the form's per-type behaviour
- `docs/src/requirements.md`: FR-TGT-09 → `Implemented`

**→ Cut v0.30.0 (M3) after subjects 21–24 are merged.**
