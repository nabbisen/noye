# Security posture

This document describes the security controls Noye implements and the threats
they address. It is the canonical reference operators should consult when
configuring a deployment, and the audit trail for what is and isn't covered.

## Threat model

Noye is a server-health monitoring application that:

- Authenticates a small number of operators (admins and members) via OIDC
- Stores monitoring configuration (targets, channels, schedules) in D1
- Issues notifications to external endpoints (webhook, email)
- Exposes a UI for inspecting monitoring history

Primary threats:

1. **Unauthorized access to the UI** — attacker reaches the Gateway and tries to read or modify config
2. **Privilege escalation** — authenticated member tries to access admin functions
3. **Phishing via gateway redirects** — attacker uses the Gateway's hostname to relay victims to a phishing site
4. **Audit log tampering** — attacker (or operator) hides their actions
5. **CSRF / clickjacking** — attacker tricks an authenticated admin's browser into submitting actions
6. **Service Binding bypass** — attacker reaches the internal Core directly, bypassing the Gateway
7. **Notification endpoint abuse** — attacker triggers test notifications as a DoS amplifier

## Implemented controls

### HTTP security headers

Every response from the Gateway carries:

| Header | Value |
|---|---|
| Content-Security-Policy | `default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; form-action 'self'; base-uri 'self'; object-src 'none'` |
| Strict-Transport-Security | `max-age=31536000; includeSubDomains` |
| X-Frame-Options | `DENY` |
| X-Content-Type-Options | `nosniff` |
| Referrer-Policy | `no-referrer` |
| Permissions-Policy | denies accelerometer, camera, geolocation, gyroscope, magnetometer, microphone, payment, usb |

Implementation: `crates/gateway/src/security_headers.rs`. The `apply()` function is invoked by every response helper (`html_response`, `redirect`, `error_response`, `with_security_headers`) so coverage is uniform across all 34 routes.

CSP `frame-ancestors 'none'` + the legacy `X-Frame-Options: DENY` together block clickjacking even in older browsers and proxies.

`'unsafe-inline'` for script-src and style-src is required by the current UI (inline scripts in `channels.rs`, `migration.rs`, `incidents.rs`; inline `style=` attributes throughout). Migrating to a nonce-based CSP is a future improvement.

### Authentication

- **OIDC Authorization Code flow** with PKCE (S256), state, and nonce — all mandatory, validated on the way in and the way out
- **No `access_token` or `refresh_token` retention** — the gateway uses opaque KV-backed sessions instead, which means the IdP's tokens cannot be exfiltrated from KV
- **session ID** is 256 bits of CSPRNG output, base64url-encoded
- **session cookie**: HttpOnly, Secure, SameSite=Lax, Path=/
- **session lifetime**: 8 hours by default (`SESSION_DURATION_MIN`), enforced both by KV TTL and by the application-layer expiry check on every request

### Authorization (RBAC)

Two roles:

- **admin** — full read/write across every resource
- **member** — read-only access to targets they own; cannot create, modify, or delete

Enforced in `crates/gateway/src/auth/rbac.rs::can_view_target` and `auth::require_admin`. All 34 routes either go through `authenticate()` (returning 302 to `/auth/login` on failure) or are intentionally public (`/healthz`, `/auth/login`, `/auth/callback`).

Future tenancy work (see `requirements.md` Roadmap) will add a `tenant_id` dimension to the RBAC check.

### Open-redirect prevention

`?return_to=` is sanitized via `crates/gateway/src/safe_redirect.rs::sanitize_return_to`. Only same-origin path-relative URLs are accepted; anything off-origin (`https://evil.example`, `//evil.example`, `/\evil.example`, anything containing CR/LF) silently falls back to `/`. Validated on entry (`/auth/login`) and again on the OIDC callback (defense in depth).

### Service Binding authentication

The Core trusts only requests carrying `X-Gateway-Token: <shared secret>`. Implementation: `crates/core/src/api.rs::verify_gateway_token_env`.

**Fail-closed**: if `GATEWAY_SHARED_TOKEN` is not configured (neither as Wrangler secret nor as `[vars]` value), every request is rejected with FORBIDDEN. This was changed from a previous fail-open behavior in 0.14.0 to prevent a misconfigured production deploy from accepting unauthenticated `X-Caller-*` headers.

Neither `wrangler.toml` is committed (Subject 03 / G-21); the `.example` templates ship no value for `GATEWAY_SHARED_TOKEN` at all. For local development, set a generated value in `.dev.vars` on both workers (git-ignored, merged in by `wrangler dev` — see [development.md](development.md)). For production, register it with `wrangler secret put GATEWAY_SHARED_TOKEN`.

### Environment-aware configuration (`NOYE_ENV`)

`NOYE_ENV` is a Gateway-only variable, governing cookie strictness.
Core does not read it — Core has no cookies or sessions of its own —
and (as of Subject 03) `NOYE_ENV` has **no bearing on dev-fallback
detection** in either Worker; that check is unconditional (below).
Implementation: `crates/gateway/src/env_check.rs`.

| `NOYE_ENV` value | Cookie `Secure` |
|---|---|
| `"development"` (case-insensitive) | dropped (allows plain-HTTP localhost) |
| anything else, or unset | required |

**Default-to-production**: an unset or unrecognized `NOYE_ENV` is treated as production, a fail-safe choice for cookie strictness if a real production deploy forgets to set the variable.

**Until 2026-07-28** this table's third column was "Dev-fallback values
accepted", gated the same way as cookie strictness — which meant the
shipped `wrangler.toml`'s own `NOYE_ENV = "development"` disabled the
dev-fallback check entirely (gap G-21, closed by Subject 03). Recorded
here rather than silently dropped from the table, since the coupling
between these two concerns was the defect.

### Leaked dev-fallback detection

Neither `crates/gateway/wrangler.toml` nor `crates/core/wrangler.toml`
is committed. The `.example` templates carry **no value** for
`OIDC_CLIENT_SECRET` or `GATEWAY_SHARED_TOKEN` — a value committed to a
template is a value published in a public repository, which is exactly
how the two literal strings below became well-known in the first place.
The templates instead point at `wrangler secret put` (deployment) and
`.dev.vars` (local development; git-ignored, merged in by `wrangler
dev`).

The denylist itself, unaffected by any of that, still needs the exact
strings ever shipped as a convenience default — including historical
ones, since a developer's pre-existing `.dev.vars` may still hold one:

```rust
OIDC_CLIENT_SECRET = "dev-idp-does-not-verify-this"
GATEWAY_SHARED_TOKEN = "noye-local-dev-shared-token"
```

If either value is observed at request time — **in any environment,
including local development** — the worker refuses to serve the
request and returns an error message that names the offending
variable, never the value. The check runs at the start of every
`fetch` event, before any other logic, and no longer consults
`NOYE_ENV` at all.

Both workers run their own check independently — see
`env_check::check_no_leaked_dev_fallbacks` in each crate, which reads
`Env` and delegates to a pure `find_leaked_fallback` function
(host-testable without a Workers runtime, NFR-QA-01). The list of
well-known fallback values is unit-tested against the values documented
in each `wrangler.toml.example`'s comments, so any future drift between
the two is caught at `cargo test` time — there is no longer a
committed `wrangler.toml` for the test to compare against directly.

### Cross-Site Request Forgery (CSRF)

Three layered defenses:

1. **OIDC flow**: protected by `state` + `nonce` (single-use, KV-stored, consumed on callback).
2. **Cookie SameSite=Lax**: strips the session cookie from cross-origin POSTs. Lax (not Strict) is required because the OIDC callback is a top-level navigation from the IdP's domain.
3. **Synchronizer Token Pattern (since 0.19.0)**: every state-changing endpoint requires an `X-CSRF-Token` request header that matches the session-bound token in KV, compared in constant time.

#### Synchronizer Token Pattern details

Implementation: `crates/gateway/src/auth/csrf.rs` (pure logic, 10 unit tests) + `crates/gateway/src/lib.rs::verify_csrf` (request-time check).

- **Issuance**. At session creation (`auth/callback`) a fresh 32-byte token is generated, base64url-encoded into 43 chars, and stored on the `Session` struct in KV alongside the session ID. Same lifetime as the session — they live and die together.
- **Surfacing**. Every authenticated HTML page renders a `<meta name="csrf-token" content="...">` tag in `<head>` (single embed point in `ui/layout.rs::wrap`). Browser-side fetch code reads it via `document.querySelector('meta[name=csrf-token]').content` and copies it into the `X-CSRF-Token` header on every state-changing request.
- **Verification**. After `authenticate()` returns a `Caller`, mutating handlers call `verify_csrf(&req, &env)`, which: 1) requires the header to be present; 2) requires the value to be 43 chars of base64url (rejects malformed before any KV read); 3) loads the session and compares its `csrf_token` to the header in constant time.
- **Coverage**. All 14 state-changing endpoints: `POST/PUT/DELETE /api/targets/...`, `POST /api/incidents/:id/resolve`, `POST /api/maintenance`, `POST/PUT/DELETE /api/channels/...`, `POST /api/channels/:id/test`, `POST/DELETE /api/targets/:id/channels/...`, `POST /api/settings/users`, `POST /api/admin/migration/import`, `POST /auth/logout`.
- **GET logout exception**. `GET /auth/logout` (link-click logout) is intentionally not CSRF-protected — an attacker who tricks a victim into a cross-site logout link can only end the session, not impersonate. The `POST /auth/logout` variant remains protected.
- **Legacy session opt-out**. Sessions issued before 0.19.0 have no `csrf_token` field. Rather than locking those users out at deploy time, `verify_csrf` allows-once with a `console_log!` warning. New sessions enforce strictly. Existing users transition the next time they re-login (typically within hours of the deploy due to the 8-hour session TTL).
- **What this does not catch**. An attacker who controls the victim's browser via XSS can read the meta tag and forge requests. CSRF tokens are defense-in-depth; the primary XSS protections live elsewhere (CSP headers, `escape_html` in the layout, parameterized SQL).

### Cross-Site Scripting (XSS)

All operator-supplied content interpolated into HTML goes through `crates/gateway/src/ui/layout.rs::escape_html`, which escapes `&`, `<`, `>`, `"`, `'`. Scripts that embed user data (e.g. channel ID in `channels.rs::render_detail_script`) use `serde_json::to_string` to produce a properly-escaped JSON literal, which is XSS-safe in JavaScript-string contexts.

### SQL injection

Every Core D1 query uses parameterized binds (`prepare("... ?1 ...").bind(&[...])`). Two acknowledged exceptions:

- `db/migration.rs::exists_by_id` interpolates the table name (which is from a fixed allowlist of literals; reviewed for safety)
- `db/retention.rs` interpolates the cutoff timestamp (which is built from `chrono::Utc::now()`, never from user input)

The `noye` binary uses string-interpolated SQL for the initial admin seed, with a strict input validator (`validate_email` in `crates/cli/src/main.rs`) that rejects emails containing single/double quotes or semicolons. CLI is local-only and the operator already has full DB access via wrangler; the validation is defense against typos rather than malicious input.

### Rate limiting

Two independent rate-limit families, both implemented in `crates/gateway/src/rate_limit.rs` over Cloudflare KV with fixed-window counters (one minute + one hour, both must pass):

| Endpoint | Scope | Default | Defends against |
|---|---|---|---|
| `/api/channels/:id/test` (test-send) | Per-channel | 5/min, 30/hour | Operator misclicking the test button repeatedly; bounded notification provider exposure |
| `/auth/login` | Per-IP (CF-Connecting-IP) | 10/min, 50/hour | Unauthenticated traffic filling KV with `pending_login:` entries (DoS / KV quota-exhaustion); brute-force OIDC state replay |

The login limit is checked **before** any KV write, so denied requests never burn quota on the very thing they were trying to abuse. Both windows must pass; the more-restrictive one is reported in the `Retry-After` header on a 429 response.

Configurable via `LOGIN_RATE_LIMIT_PER_MIN` / `LOGIN_RATE_LIMIT_PER_HOUR` on the Gateway. A misconfigured value (non-numeric or zero) silently falls back to the default rather than failing the request — operators should never be locked out by a typo.

**Other unauthenticated endpoints** (`/healthz`, `/auth/callback`) are not rate-limited:
- `/healthz` returns a static string with no KV writes
- `/auth/callback` requires a valid `state` parameter that maps to a KV `pending_login:` entry, which itself is rate-limited by the corresponding `/auth/login`

**Not yet rate-limited**:

- `/api/admin/migration/import` — large-impact admin operation. Authentication-gated, so the practical risk is low, but bursts could still strain D1 writes.

### Cookie attributes

| Attribute | Value | Reason |
|---|---|---|
| HttpOnly | yes | Block access from `document.cookie` (XSS doesn't get the session) |
| Secure | yes (in production); dropped in development | Prevent transmission over plain HTTP. The `Secure` attribute is conditioned on the `NOYE_ENV` value: dropped only when `NOYE_ENV = "development"` so plain-HTTP `wrangler dev` round-trips correctly. Production deploys (anything else, including unset) keep `Secure` on. |
| SameSite | Lax | CSRF defense; Lax (not Strict) is needed because the OIDC callback is a top-level navigation from the IdP's domain |
| Path | `/` | Single cookie scope for the whole gateway |

The `Secure` flag is set by `crates/gateway/src/auth/session.rs::create` based on `env_check::Environment::from_env`, which reads the required `NOYE_ENV` env variable. Cookie clearing (`clear_cookie`) mirrors the same flag so the browser correctly matches the original cookie when erasing it.

### Bot protection (Turnstile)

Cloudflare Turnstile scaffolding exists (`crates/gateway/src/auth/turnstile.rs`) but is not currently wired into any route. When applied (planned for `/auth/login` and any future signup flow), it will check the `cf-turnstile-response` token via the siteverify API.

### Audit logging

Every admin write is recorded in the `audit_logs` D1 table:

- `id` (UUID), `action_time` (UTC), `actor_id`, `actor_email` (denormalized so deletions don't lose context), `resource_type`, `resource_id`, `action_type`, `previous_value` (JSON), `new_value` (JSON), `result`, `ip_address`
- `prev_hash`, `row_hash` (since 0.27.2) — see "Audit log tamper detection" below

#### Audit log tamper detection

Each audit row carries a `row_hash` computed as

```
row_hash[N] = SHA256(prev_hash[N-1] || canonical_serialization(row[N]))
```

where `prev_hash[N-1]` is the previous row's `row_hash` and `canonical_serialization` is a deterministic byte encoding of the row's identifying fields. **The chain's order is insertion order, carried entirely by the `prev_hash → row_hash` links** — not recovered by sorting on any stored column (subject 05, DEC-020; see below). Tampering with any prior row — `UPDATE`, `DELETE`, or out-of-order insertion — invalidates every later row's `row_hash`.

Implementation: `crates/core/src/db/audit/hash.rs` (pure logic, 21 unit tests pinning the format). The chain is built incrementally: each `INSERT` reads the current chain head and includes its `row_hash` as the new row's `prev_hash`. Genesis (the very first row) uses 64 hex zeros.

**Verification endpoint**: `GET /api/admin/audit/verify` (admin-only) loads the entire table and walks it by following each row's `prev_hash → row_hash` link from genesis — never by sorting rows into an order and assuming adjacency-in-sort means adjacency-in-chain. It recomputes each reached row's `row_hash` and reports the result in four classes:

```json
{
  "total_rows": 1234,
  "legacy_rows": 5,
  "verified_rows": 1226,
  "tampered_rows": [
    {"id": "abc...", "action_time": "...", "reason": "row_hash does not match recomputed value (row contents tampered)"}
  ],
  "orphaned_rows": [
    {"id": "def...", "action_time": "..."}
  ]
}
```

`legacy_rows` are rows written before 0.27.2 with NULL hash columns; their absence of a chain is expected, not tampering. `orphaned_rows` are rows carrying hash columns that were never reached from genesis — typically because the row before them was deleted, or (rarely) because two rows raced to chain from the same head. **`orphaned` is reported separately from `tampered` and must never be collapsed into it**: a tampered row was itself altered, while an orphan is usually intact evidence that some *other* row was removed — conflating them names the wrong row as damaged.

**What this catches**: an attacker (or operator) editing or deleting historical audit rows via `wrangler d1 execute` or any other D1 access path. Editing a row's content is reported as that row, and only that row, `tampered` — its successors are still reached and verified normally, since forward-linking depends only on the edited row's unchanged stored `row_hash`. Deleting a row makes every row chained after it unreachable from genesis, reported as `orphaned` — visible from the deletion point through to the present, without misnaming those later rows as themselves altered.

**Order recovery, not data repair.** Rows chained before subject 05 under a same-second tie may already be stored in an order the old, sort-based verifier misread as broken. Following the links instead **repairs the reading, not the data**: no stored `prev_hash` or `row_hash` is rewritten by this change, and it never will be — recomputing or rewriting stored hashes to force a chain to verify would defeat the entire property this mechanism asserts. A chain that read as damaged before this fix and reads as intact after it was never actually damaged; the verifier was.

**What this does not catch**: an attacker who controls the live worker code (they could rewrite the hashes alongside the rows), or who deletes the entire table (no chain to compare against). The first is mitigated by Cloudflare's deploy access controls; the second by routine off-system mirroring (Cloudflare Logs export — operator-configured).

**Concurrency caveat**: two concurrent writers can read the same chain head and produce a fork. Normal Noye operation is single-writer (cron is single-fiber, admin API is one human user), so this is acknowledged but not currently mitigated. A future Workers Queue fan-out would need a Durable Object for serialization.

### Self-service security page (`/me/security`)

Available to any authenticated user since 0.20.0. Surfaces:

- **Account fields** — email, display name, role.
- **Current session** — issued time, expires time, CSRF protection state. A "Log out of this session" link (alias for `/auth/logout`) is included so the page is a one-stop shop.
- **Other active sessions** — a table of every other session for the same `user_email`, sorted newest first. Includes a "Log out of all other sessions" button that calls `POST /api/me/sessions/revoke-others` (CSRF-required), which destroys every session for the user except the one issuing the request.
- **Recent logins** — the calling user's last 20 `login` events from `audit_logs`. Records are written by the OIDC callback right after a fresh session is minted (Service-Binding-token-authenticated `POST /audit/login` on the Core).
- **Audit log integrity** (admin only) — a button that fetches `/api/admin/audit/verify` and displays the result inline. Non-admins do not see this card.

The session enumeration is a best-effort `KV.list({prefix: "session:"})` followed by per-key reads, filtered by `user_email`. Hard-capped at 1000 keys per page (KV's limit); for the expected scale (< 100 active sessions globally) this is sufficient. If a deployment ever exceeds that, a per-user index `user_sessions:<email>` would be the upgrade path. Implementation: `crates/gateway/src/auth/session.rs::list_active_for_user`.

Force-logout is best-effort across the listed sessions: a transient KV `delete` failure on one session does not block the others. The pure logic for "exclude the current session from the revoke set" is in `ids_to_revoke_excluding_current` (5 unit tests covering: normal exclusion, current-not-found falls through to revoke-all, empty input, only-current input, input-order preservation).

What this page **does not** provide:

- IP / user-agent labelling for old sessions. KV's session entries do not carry that data; adding it would require a schema bump and a migration. Decision: defer until there is real operator demand.
- Session "rename" / human label. Same reason.
- A live-tail of failed login attempts. The OIDC callback today only logs *successful* logins to audit. Failed attempts log to `console_error!` but are not audit-rowed. Worth doing later in tandem with a `/auth/login` failure-classification refactor.

## Dependency posture

The supply chain is audited with [`cargo-audit`](https://rustsec.org/),
which compares the workspace `Cargo.lock` against the
[RUSTSEC advisory database](https://github.com/RustSec/advisory-db).
The audit job is wired into CI on every push and PR plus a weekly
schedule (so advisories published after a quiet period still surface).
See `.github/workflows/ci.yml`.

### Current status (0.27.1)

A full scan of the 0.27.1 lockfile (223 unique `(name, version)` pairs
× the 2026-05 RUSTSEC database) found:

- **0 confirmed CVE exposures.** Every dependency that has any
  advisory at all is in the advisory's patched range.
- **1 documented suppression** — see below.
- **0 unmaintained / informational notices.**

### Documented suppression: RUSTSEC-2023-0071 ("Marvin Attack" in `rsa`)

[RUSTSEC-2023-0071](https://rustsec.org/advisories/RUSTSEC-2023-0071) /
CVE-2023-49092 / GHSA-c38w-74pg-36hr (CVSS 5.9, Medium) describes a
non-constant-time RSA implementation that leaks key bits via timing.
Upstream has no patched release as of 2026-05 — constant-time work is
in progress at [RustCrypto/RSA#19](https://github.com/RustCrypto/RSA/issues/19).

In Noye the `rsa` crate is reachable **only** from `noye-dev-idp`, the
local OIDC stub:

| Property | Value |
|---|---|
| Crate that uses `rsa` | `noye-dev-idp` (host binary, never a Worker) |
| Bind address | `localhost` only (default port `5556`) |
| Production deployment | None — production uses a real IdP |
| Key persistence | None — fresh RSA-2048 per restart, in-memory |

The Marvin Attack threat model requires the attacker to observe
signing-operation timing — over a network or as a co-located process.
A `noye-dev-idp` instance running on `localhost` of a developer's own
machine satisfies neither. If a developer's machine is compromised to
the point that another process on it can time local socket I/O, this
sidechannel is not the security-critical leak.

The advisory is therefore explicitly ignored in `.cargo/audit.toml`
with a justification that points back to this section. **Reassess when
upstream `rsa` ships a constant-time fix** — at that point the workspace
dep should bump and the ignore entry should drop.



These are explicitly tracked as deferred work in `requirements.md`:

| Area | Gap | Mitigation |
|---|---|---|
| Audit log off-system mirror | Hash chain detects tampering, but a wholesale `DROP TABLE` leaves nothing to verify against | Cloudflare Logs export (operator-configured) |
| Refresh token theft detection | Not applicable — Noye does not retain refresh tokens | — |
| Multi-tenant isolation | Single-tenant deployment | Run separate deployments per tenant for now |

See [`requirements.md`](requirements.md) for the full roadmap.

## Operator checklist

Before going to production:

- [ ] Copy `crates/gateway/wrangler.toml.example` → `wrangler.toml` and `crates/core/wrangler.toml.example` → `wrangler.toml` if you haven't already (git-ignored; neither carries a secret value, and Gateway's already ships `NOYE_ENV = "production"` — Core does not read `NOYE_ENV` at all, so there is nothing to switch or remove on either)
- [ ] `wrangler secret put OIDC_CLIENT_SECRET` with the real IdP client secret
- [ ] `wrangler secret put GATEWAY_SHARED_TOKEN` on **both** Gateway and Core, with the same value
- [ ] Configure `OIDC_REDIRECT_URI` to the production gateway URL (`https://...`)
- [ ] Confirm HTTPS is enforced (Cloudflare default; no plaintext HTTP listener)
- [ ] Enable Cloudflare WAF and rate limiting on the Gateway zone
- [ ] Review `crates/gateway/wrangler.toml` — `OIDC_ISSUER_URL` is your production IdP, not `http://localhost:5556`
- [ ] Configure D1 / R2 retention policies appropriate to your compliance requirements
- [ ] Enable Cloudflare Logs export (audit log mirror) if regulatory requirements demand off-system retention
- [ ] Consider Turnstile integration for the production login page (currently scaffolded but not wired up)
