# RFC 0002: Cloudflare Logs export — audit-log mirror

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-05-04
**Related ROADMAP item**: "Cloudflare Logs export (audit-log mirror)" under `## Operations infrastructure`
**Estimated size**: medium (operator-side runbook + minor Worker changes)
**Implementation target**: post-0.27.x

---

## Summary

Provide an off-D1 mirror of every `audit_logs` row using Cloudflare
Logpush (or equivalent log-shipping). The in-D1 hash chain detects
tampering of existing rows; this RFC closes the wholesale-deletion gap
("`DROP TABLE audit_logs`") by ensuring every row is also written to
an external append-only sink. The mirror is the recovery source if the
D1 table is ever destroyed.

## Background

The audit-log hash chain (0.18.0) gives us tamper-evidence: any
modification, reordering, or partial deletion produces rows whose
recomputed `row_hash` no longer matches their stored value, which
`GET /api/admin/audit/verify` surfaces as `tampered`.

The chain does not cover *complete* destruction. If the entire
`audit_logs` table is dropped or replaced with a forged-from-genesis
chain, verify reports "intact" with whatever the new content claims.
This is the threat the off-D1 mirror addresses.

## Design

### Architecture

The mirror is **operator-configured at the Cloudflare level**, not in
Noye code. Noye's contribution is:

1. A small change in Core that makes every audit-log write also emit a
   structured log line (one JSON object per `console_log!`) with a
   stable schema, so Logpush has something machine-parseable to ship.
2. Documentation: a runbook covering Logpush configuration, the
   destination's retention guidance, and the end-to-end verify-and-
   restore procedure.

### Log line schema

A single line per audit-log row, emitted by `noye-core` immediately
after the row is committed to D1 (after the `INSERT` succeeds — never
before, so we never log an event that didn't actually happen):

```json
{
  "kind": "audit_log_row",
  "schema_version": 1,
  "id": "...",
  "action_time": "2026-05-04T12:34:56.789Z",
  "actor_id": "...",
  "actor_email": "alice@example.com",
  "resource_type": "target",
  "resource_id": "tgt-...",
  "action_type": "update",
  "previous_value": "{...JSON...}",
  "new_value": "{...JSON...}",
  "result": "success",
  "ip_address": "203.0.113.5",
  "prev_hash": "...",
  "row_hash": "..."
}
```

The line is emitted via `console_log!` (which becomes a Workers Trace
Event). Logpush includes the message verbatim in the destination
stream. The `kind` discriminator lets the operator filter just the
audit-log rows from the mixed Trace stream.

### Destination

The runbook documents three destinations the operator can pick from
based on existing infrastructure:

- **Cloudflare R2** — simplest, same-vendor, JSON-Lines per file.
  Recommended default.
- **AWS S3 / GCS** — when the operator already runs cross-vendor
  observability.
- **Datadog / Splunk / etc.** — when the operator already has a SIEM.

Logpush is the shipping mechanism in all three cases (it has built-in
adapters for each). The runbook only documents R2 in detail; the
others are referenced.

### Worker-side change

Add `noye_core::audit::emit_log_line(&entry)` called from
`audit::insert_*()` immediately after the SQL `INSERT` returns. The
helper produces the JSON above with `kind: "audit_log_row"` and
`schema_version: 1` and writes it via `console_log!`. The function is a
no-op error-wise — failure to log must not cause the original action
to fail (the audit-log row is already in D1; the off-system mirror is
best-effort durable).

This is ~30 lines of code plus tests.

### Restore procedure

Documented in the runbook. Outline:

1. Stop writes (set Cron Trigger to a paused state, take maintenance
   window).
2. Drain the Logpush stream up to the most recent line.
3. Reconstruct `audit_logs` from the JSON-Lines mirror, ordered by
   `action_time` (which corresponds to chain position).
4. Run `GET /api/admin/audit/verify` against the restored table and
   confirm chain integrity.
5. Re-enable writes.

The procedure assumes the Worker is healthy enough to run verify; if
not, the operator can run verify offline against the D1 export using a
small standalone script (also documented in the runbook).

## Requirements

- Every successful `audit_logs` `INSERT` MUST emit exactly one log
  line with `kind: "audit_log_row"` and the schema above.
- Failure to emit the log line MUST NOT fail the parent action — the
  D1 row is already authoritative.
- The schema MUST carry `schema_version: 1` so future changes are
  detectable by the consumer.
- The runbook MUST document Logpush configuration end-to-end for the
  R2 destination at minimum.
- The runbook MUST document the restore procedure including how to
  verify chain integrity post-restore.
- The schema MUST be additive across versions: new fields can be added
  to `schema_version: 1`; breaking changes bump the version.

## Test plan

### Host unit tests (target: `noye_core::audit::log_emit`)

- `emit_log_line_includes_all_audit_fields_in_canonical_order`.
- `emit_log_line_marks_kind_audit_log_row_and_schema_version_1`.
- `emit_log_line_serializes_None_fields_as_json_null_not_omitted`.
- `emit_log_line_does_not_panic_on_missing_optional_fields`.

### Integration test

- A miniflare-based test that calls `audit::insert_create()` with a
  fake target row and asserts a log line of the documented shape was
  produced (read from the captured Workers `console_log!` output).

### Manual / smoke

- Stand up Logpush against R2 in the staging environment, run a few
  audit-producing operations, and verify the resulting JSON-Lines
  files contain exactly the expected rows.
- Run the documented restore procedure end-to-end against staging.

## Security considerations

- **PII in the mirror.** The log lines carry actor email, IP, and
  serialized previous/new values that may include resource names. The
  destination has the same sensitivity as the audit log itself; the
  runbook flags this and recommends matching access controls.
- **Trust boundary.** The log line is the *only* off-system source of
  truth in the recovery scenario. Anyone who can write to the
  destination can forge audit history at restore time. The runbook
  must call out that destination write access should be limited to
  Cloudflare's service account.
- **Replay during restore.** A malicious operator could selectively
  drop log lines during restore. This is a residual risk; the only
  full mitigation is a tamper-evident off-system store (write-once
  bucket, blockchain-style anchoring), which is out of scope here.
  The runbook documents the residual.
- **Log size / cost.** A high-traffic deployment may produce many
  audit rows; the operator-side cost of the destination grows
  linearly. The runbook recommends retention bounds (default 1 year)
  matching the in-D1 retention policy.

## Out of scope

- A built-in Worker that periodically polls the destination and
  cross-checks `audit_logs` content. Useful but separate.
- Cryptographic anchoring (timestamping service, blockchain) of the
  mirror.
- Multi-destination fan-out (Logpush already supports it; the runbook
  just points at Cloudflare's docs).
- Custom destinations that aren't Logpush-supported.

## Migration / rollout notes

- No D1 schema change.
- The Worker code change is additive; existing audit rows in D1 are
  not retroactively shipped (they predate Logpush). The runbook calls
  this out and offers a one-shot script that exports existing D1
  contents to JSON-Lines and uploads them to R2 with the same shape,
  to bring history into the mirror.
