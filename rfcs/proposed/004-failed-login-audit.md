# RFC 0004: Failed-login audit recording

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-05-04
**Related ROADMAP item**: "Failed-login audit recording" under `## Feature`
**Estimated size**: medium
**Implementation target**: post-0.27.x

---

## Summary

Persist failed `/auth/login` attempts to `audit_logs` so they appear
in `/audit` and (when the failed actor is identifiable) in
`/me/security` recent-logins. Today these failures only land in
`console_error!` and are invisible to operators. The hard part is not
the writes themselves but deciding *what to attribute the row to* when
the failing actor may not correspond to a real user.

## Background

The OIDC callback `gateway::auth::oidc::handle_callback()` records a
`login` audit row only on the successful path. Failures branch off
into `console_error!` and a redirect to the login page with an error
flash. Reasons a login can fail:

| Failure point | What we know about the actor |
|---|---|
| OIDC `state` mismatch | Nothing — could be CSRF, could be browser tab race |
| OIDC token exchange failure | Nothing — IdP error, network, etc. |
| ID token signature invalid | The IdP-claimed `sub` (untrusted) |
| User not in `users` table | The IdP-verified `sub` and `email` |
| User `is_active = false` | The IdP-verified `sub` and `email`, plus a known `users.id` |
| RBAC role mapping failure | The IdP-verified `sub` and `email`, plus a known `users.id` |

The first two cases have no actor at all; the others have varying
amounts of identity. The audit-log row schema requires `actor_id`,
which forces a decision.

## Design

### Failure classification

A new enum in `noye_shared::auth`:

```rust
pub enum LoginFailure {
    StateMismatch,              // CSRF / tab race
    TokenExchangeFailed,        // IdP error / network
    IdTokenInvalid,             // Untrusted sub
    UserNotRegistered,          // Verified sub + email, but unknown
    UserDeactivated,            // Verified sub + email, known user, is_active=false
    RoleMappingFailed,          // Verified, registered, active, but role unresolvable
}
```

Each variant carries the data we *do* know:

```rust
pub struct LoginFailureContext {
    pub kind: LoginFailure,
    pub claimed_email: Option<String>,    // IdP-claimed, may be untrusted
    pub claimed_sub: Option<String>,      // IdP-claimed, may be untrusted
    pub verified_user_id: Option<String>, // Only when sig+sub verified and user exists
    pub ip_address: Option<String>,
    pub error_detail: String,             // For the audit row's new_value JSON
}
```

### Actor attribution rules

| `LoginFailure` variant | `actor_id` | `actor_email` |
|---|---|---|
| `StateMismatch` | `"system"` | `None` |
| `TokenExchangeFailed` | `"system"` | `None` |
| `IdTokenInvalid` | `"system"` | `claimed_email` (clearly marked untrusted in `new_value`) |
| `UserNotRegistered` | `"system"` | `claimed_email` |
| `UserDeactivated` | `verified_user_id` | the deactivated user's email |
| `RoleMappingFailed` | `verified_user_id` | the user's email |

Rationale:

- The `audit_logs.actor_id` is a string column with no foreign-key
  constraint, so `"system"` is already used for system-initiated
  events. Reusing it for "no identifiable actor" failures keeps the
  schema clean.
- We never put a `claimed_email` in `actor_id` — the column is
  consumed by joins and downstream queries that expect it to point at
  a real `users.id` or the literal `"system"`.
- The IdP-verified user-id is OK to use as `actor_id` only when we've
  validated the ID token signature. For the variants that haven't
  verified that yet (`StateMismatch`, `TokenExchangeFailed`,
  `IdTokenInvalid`), `actor_id` MUST be `"system"`.

### Audit row shape

A new `action_type` value: `login_failed`. The row carries:

| Column | Value |
|---|---|
| `action_type` | `"login_failed"` |
| `result` | `"failure"` |
| `actor_id` | per the table above |
| `actor_email` | per the table above |
| `resource_type` | `"login"` |
| `resource_id` | `None` |
| `previous_value` | `None` |
| `new_value` | JSON: `{"kind": "...", "claimed_email": "...", "claimed_sub": "...", "error_detail": "...", "trusted": false}` for unverified failures, `{"trusted": true, ...}` when sub was verified |
| `ip_address` | from `CF-Connecting-IP` header |

The `trusted` flag in `new_value` lets `/audit` viewers distinguish
"we know this attempt came from this person" from "an attacker
claimed to be this person."

### `/audit` page changes

`gateway::ui::audit::action_label` gains the `login_failed` case
returning `("login_failed", "login failed")`. The row renders with
`badge-down` for the result column, like other failures.

### `/me/security` recent-logins changes

The recent-logins query in
`noye_core::audit::list_login_history_for_user` already filters by
`actor_id`. With the new variants, the query continues to work for
verified failures (`UserDeactivated`, `RoleMappingFailed`) where the
row carries the real `actor_id`. The unverified variants
(`actor_id = "system"`) won't appear in any user's recent-logins,
which is correct — we don't have a verified attribution to attach
them to.

The page header copy MUST be updated to clarify that "Recent logins"
shows successful logins and *failures attributable to your account*,
to set the expectation that some attempted-impersonations may not
appear here.

### Hash-chain compatibility

The hash chain consumes the `audit_logs` row as it exists; new
`action_type` values flow through unchanged. The
`gateway::core::db::audit::hash` test that pins canonical
serialization should add a fixture exercising `login_failed` to lock
in the hash for that variant.

### Rate limiting and storage

A failed-login storm could fill `audit_logs`. The existing IP rate
limit on `/auth/login` (10/min, 50/hour) is the primary defense; this
RFC does not change it. The `audit_logs` retention policy applies as
usual (audit logs are not auto-pruned today; see RFC 0002 for the
mirror that lets long retention be safer).

## Requirements

- Every `/auth/login` failure MUST produce exactly one `audit_logs`
  row with `action_type = "login_failed"`.
- The `actor_id` attribution rules in the table above MUST be honoured
  exactly. A unit test pins the mapping for each `LoginFailure`
  variant.
- The `new_value` JSON MUST carry a boolean `trusted` field that is
  `true` only when the ID token signature was successfully verified
  for the claimed `sub`.
- The hash-chain test suite MUST be extended with a `login_failed`
  fixture so future schema work cannot accidentally change its
  canonical bytes.
- A pure helper `attribution_for(failure: &LoginFailureContext) ->
  AuditRowAttribution` MUST be unit-testable on the host target.
- `/me/security` recent-logins MUST surface `UserDeactivated` and
  `RoleMappingFailed` events for the affected user, and MUST NOT
  surface unverified-sub variants.
- The `/audit` page MUST render `login_failed` rows with a recognised
  badge and the existing diff-disclosure UI from Phase D.
- An IdP-claimed email or sub MUST NEVER end up in the `actor_id`
  column. A regression test enforces this on the
  `attribution_for` mapping.

## Test plan

### Host unit tests (target: `gateway::auth::oidc::failure`)

- One test per `LoginFailure` variant for `attribution_for()`,
  asserting the exact `(actor_id, actor_email, trusted)` triple.
- `actor_id_is_never_a_claimed_email_or_sub_for_any_variant` — the
  regression-guard test.
- `new_value_serialization_includes_trusted_flag`.

### Host unit tests (target: `gateway::core::db::audit::hash`)

- `canonical_serialization_for_login_failed_row_is_pinned` — fixture
  test that locks the byte-level shape of a `login_failed` row's
  hash input. Failing this test means a contributor has changed the
  canonical layout, which would break the chain.

### Host unit tests (target: `gateway::ui::audit`)

- `action_label_renders_login_failed_with_failure_badge`.
- `unknown_login_failed_actor_renders_as_system_in_actor_column`.

### Manual / smoke

- Run staging against a misconfigured OIDC client to provoke
  `TokenExchangeFailed` and confirm the audit row appears with
  `actor_id = "system"`, `trusted = false`.
- Deactivate a real user and have them log in; confirm the row
  appears and shows up on their `/me/security` page with `trusted =
  true`.

## Security considerations

- **Attacker-controlled fields in audit log.** The `new_value` for
  unverified failures carries IdP-claimed strings the attacker
  influences. The `trusted: false` flag and the JSON-only persistence
  (no SQL injection vector — `actor_email` is bound parameterized;
  `new_value` is a JSON column) keep this safe to display.
- **Information disclosure on `/audit`.** A registered user's email
  showing up next to `UserDeactivated` reveals that the email is a
  registered (deactivated) account. This is acceptable —
  `/audit` is admin-only — but documented in
  `docs/src/security-posture.md`.
- **Log poisoning.** An attacker can produce many
  `login_failed` rows with arbitrary claimed emails. The IP rate limit
  bounds the rate per-source. The `trusted: false` flag means downstream
  log analysis won't confuse these with real user activity.
- **Hash-chain dependency.** New row variant flows through the
  existing chain code without modification. The pinned serialization
  test prevents future drift.

## Out of scope

- Per-user lockout after N failed attempts (separate concern; ROADMAP
  item B-3).
- Surfacing failed logins on a notifications channel.
- A `/audit` filter UI for `login_failed` rows specifically — the
  general filtering on `action_type` would cover it but is itself out
  of scope.
- Counting failed-login attempts as a metric exposed on `/stats` —
  Noye is not a metrics tool.

## Migration / rollout notes

- No D1 schema change; `action_type` is a free-form `TEXT` column.
- Existing `audit_logs` rows are unaffected.
- `/audit` and `/me/security` will simply start showing the new rows
  on the next monitor tick after deploy.
