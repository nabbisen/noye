# 07b — Every boolean read from D1 traps the Worker

**Milestone** M1.1 · **Closes** G-36 · **Blocks** 07a #2/#3/#4
**Branch** `fix/07b-d1-bool` · **Depends on** nothing
**Work this before anything else.**
**Governing artifact** — Gap **G-36** (§11)

## The defect

SQLite has no boolean type. Every `bool` in a struct D1 deserializes into
is backed by an `INTEGER` column, D1 surfaces it as a JS number, and
`serde_wasm_bindgen` does not coerce a number into a Rust `bool`.

The `worker` crate does not give you a chance to handle it
(`worker-0.8.5/src/d1/mod.rs`):

```rust
let result = serde_wasm_bindgen::from_value(result).unwrap();
```

**`.unwrap()` — so the mismatch is a Wasm trap, not a returned error.**
`?` cannot propagate it. No `console_error!` fires. A real deployment
aborts the invocation with no application-level log line, which is why
`monitor/engine.rs`'s retention error handler has never once reported
this.

### Seven fields, six structs, six tables

| Struct | Field | Column |
|---|---|---|
| `User` | `is_active` | `users.is_active INTEGER NOT NULL DEFAULT 1` |
| `Target` | `is_disabled` | `targets.is_disabled INTEGER NOT NULL DEFAULT 0` |
| `CheckResult` | `is_success` | `check_results.is_success INTEGER NOT NULL` |
| `MaintenanceWindow` | `is_active` | `maintenance_windows.is_active INTEGER NOT NULL DEFAULT 1` |
| `MaintenanceWindow` | `suppress_notify` | `maintenance_windows.suppress_notify INTEGER NOT NULL DEFAULT 1` |
| `NotificationChannel` | `is_enabled` | `notification_channels.is_enabled INTEGER NOT NULL DEFAULT 1` |
| `RetentionPolicy` | `archive_to_r2` | `retention_policies.archive_to_r2 INTEGER NOT NULL DEFAULT 0` |

**The service has never worked against D1.** Not the retention pass — the
data layer. This is not a regression; it predates every release.

## ⛔ Step 0 — reproduce, then measure the blast radius

1. **Reproduce one case end-to-end** against `wrangler dev --local`, as
   subject 07a's investigation did. You have the method already.
2. **Then prove the other six are the same defect and not six
   assumptions.** One reproduction plus a table is not evidence for seven
   fields. The cheapest honest form is a host test per field asserting
   that `serde_wasm_bindgen::from_value::<T>(<a JS number>)` fails for
   `bool` — that is the whole mechanism, and it needs no D1.

**If any of the seven turns out not to reproduce, stop and report.** A
field that works would mean the mechanism is not what this document says,
and everything below would be built on a wrong premise.

## Build

### The design decision is yours to propose, mine to ratify

Three mechanisms. **Report which you propose and why, before building
it** — this changes types that cross the Gateway↔Core boundary, appear in
the JSON API, and appear in CSV exports.

| | Mechanism | Cost |
|---|---|---|
| **A** | A `deserialize_with` helper accepting number, bool, or string | Seven attribute lines; struct types unchanged; the API and CSV surfaces are untouched |
| **B** | `#[serde(from = "i64")]` on a newtype | More machinery; same outcome |
| **C** | Change the fields to `i64` and adapt call sites | Honest about the storage; **changes the JSON API and every consumer**, and pushes truthiness into business logic |

**I expect A**, because it is the only one that fixes the deserialization
without changing what the rest of the system sees. But the decision is
worth stating rather than assuming, and if reproduction turns up something
that rules it out, say so.

**Whichever you pick, it must accept a genuine `bool` too.** Nothing
guarantees D1 will always surface these as numbers, and a helper that
breaks if it ever returns a real boolean would trade this defect for a
subtler one.

### Do not

- **Do not change the schema.** `INTEGER` is how SQLite stores booleans;
  the columns are correct and `DR-MIG-02` forbids altering released
  migrations regardless.
- **Do not fix only `RetentionPolicy`** because it is the one that was
  observed. All seven, or the defect stays live in six places.
- **Do not vendor or patch the `worker` crate.** Its `.unwrap()` is
  upstream's to fix; ours is to give it data it can deserialize.
- **Do not widen this into "audit every D1 struct field type."** Integers,
  strings and options round-trip. Booleans are the defect. If you believe
  another type has the same problem, report it — do not fold it in.

## Verify

| # | Test | Type |
|---|---|---|
| T-189 | Each of the seven fields deserializes correctly from a JS **number** — one assertion per field, named for the field | **must fail first** |
| T-190 | …and from a genuine JS **boolean**, so the fix is not one-directional | guard |
| T-191 | `0` → `false` and `1` → `true` for every field. A helper that returns `true` for `0` would pass T-189 | **guard — critical** |
| T-192 | `run_cleanup` completes a full pass against the local D1 runtime — the thing that has never happened | **must fail first** |
| T-193 | One typed read per affected table succeeds against local D1: `users`, `targets`, `check_results`, `maintenance_windows`, `notification_channels` | **must fail first** |

**T-191 is the one that matters.** The failure mode of a coercion helper
is not "it doesn't compile" — it is a silent inversion. `is_disabled`
reading `true` for every target would disable all monitoring, and every
other test here would still pass.

**T-192 and T-193 need the local D1 runtime**, and are the first tests in
this project's history to assert that reading a row works at all.

## Done

- All five tests pass; T-189's, T-192's and T-193's baselines captured
- `docs/src/requirements.md`: G-36 struck, with the mechanism recorded
- `CHANGELOG.md` — and note it publishes verbatim now
- **07a's #2, #3, #4 unblocked** and handed back

## Escalate

- **Any of the seven not reproducing** → architect, before building.
- **A mechanism other than A** → architect, with the reasoning, before
  building.
- **Anything that suggests a non-boolean type is also affected** →
  architect. Report it; do not absorb it.
