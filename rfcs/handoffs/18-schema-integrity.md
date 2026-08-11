# 18 — Schema constraints, timestamps and indexes

**Milestone** M2 · **Closes** G-13, G-14, G-15 · **Satisfies** DR-INT-01, 02, 03, 07, 08
**Branch** `fix/18-schema-integrity` · **Depends on** subject 17 (same migration `0009`)
**Governing artifact** — Gaps **G-13**, **G-14**, **G-15** (§11)

## The defects

The guiding rule, from the requirements: **application-level validation
is necessary but not sufficient.** Data reaches these tables through the
API, the CLI, configuration import, and direct database access. A
constraint that exists only in Rust holds for one of those four.

**G-13** — no boolean, range or interval constraints anywhere.

**G-14** — schema defaults use `datetime('now')`, producing
`YYYY-MM-DD HH:MM:SS`; the application writes RFC 3339
`YYYY-MM-DDTHH:MM:SSZ`. Scheduling and window-overlap logic compare these
**as strings**, and `' '` (0x20) sorts below `'T'` (0x54). Mixed formats
compare *incorrectly but silently* — the worst failure mode available.

**G-15** — several live access paths lack indexes.

**These constraints are final.** DEC-008 settled that no tenant column is
coming, so nothing here will be reshaped later.

## Build — migration `sql/0009`

> **Renumbered 2026-08-11, before any migration was written.** This was
> `0007`. Subjects 15 and 16 also need migrations and had none reserved;
> since migrations apply in filename order and these subjects are worked
> 14 → 18, 15 takes `0007`, 16 takes `0008`, and 17/18 move to `0009`.
> `0005`–`0007` were reservations only — no file beyond `0004` exists —
> so this is a reservation change, not a renumbering of anything used.

### Constraints

- Every boolean column: `CHECK (col IN (0,1))`
- `port` 1–65535 · `expected_status` 100–599 · `timeout_sec` 1–300 ·
  `retry_count` 0–10 · `interval_minutes` 1–1440 · `tls_threshold_days >= 0`
- `success_threshold` and `failure_threshold` each `BETWEEN 1 AND 10`.
  **Zero must not be representable** — it would mean "transition on no
  evidence"
- `maintenance_windows`: `CHECK (start_at < end_at)`, closing the schema
  half of FR-SUP-10

Tables needing a CHECK added require the table-rebuild procedure.

### Timestamps

1. Replace every `DEFAULT (datetime('now'))` with
   `DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ','now'))`.
2. Normalise existing rows in the same migration.

> `stats.rs` carries a defensive parser accepting both forms. That helps
> read paths and does nothing for `WHERE` comparisons, which is where the
> damage is. Do not treat it as mitigation.

### Indexes

At minimum: `notification_channels(owner_id)`; the reverse
`target_notifications(channel_id)` lookup — the primary key is
`(target_id, channel_id)`, so channel→targets currently scans;
window-overlap lookups; `audit_logs(action_type)`.

Subject 12's `target_tags(tag)` index already closed part of this.

## Verify

| # | Test | Type |
|---|---|---|
| T-87 | Every boolean column rejects a value other than 0 or 1 | **must fail first** |
| T-88 | Each numeric range rejects one value below and one above its bound | **must fail first** |
| T-89 | Thresholds reject 0 and reject 11 | **must fail first** |
| T-90 | A window with `end_at <= start_at` is rejected by the database | **must fail first** |
| T-91 | A row written by schema default and one written by the application sort identically for the same instant | **must fail first** |
| T-92 | Valid values at **each boundary** are accepted — port 1 and 65535, timeout 1 and 300, retries 0 and 10 | guard |
| T-93 | Every listed access path is index-supported | guard |

**T-88 and T-92 are a pair.** A constraint written slightly wrong —
`BETWEEN 1 AND 65534`, an off-by-one — rejects valid data, and only the
accepting test catches it. Test both edges, in both directions.

**T-91 is the subtle one.** Build the fixture as two rows for the same
instant, one via schema default and one via the application, and assert
they order together. Do not assert on the string format — assert on the
comparison, because the comparison is what the scheduler depends on.

## Done

- All seven tests pass; five baseline failures captured
- `docs/src/requirements.md`: DR-INT-01, 02, 03, 07, 08 → `Implemented`;
  G-13, G-14, G-15 struck

## Escalate

A constraint rejecting rows already present in a live database → report
before anyone corrects the data. It means production data violates a rule
we are about to enforce, and that is worth understanding first.
