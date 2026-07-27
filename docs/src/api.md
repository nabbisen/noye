# API reference

## Gateway external API

These routes are reachable from the public Internet. All non-`/auth/*` and non-`/healthz` routes require a valid session cookie; unauthenticated requests are redirected to `/auth/login?return_to=<original_path>`.

**CSRF protection (since 0.19.0)**: every state-changing request below (`POST` / `PUT` / `DELETE`, except `GET /auth/logout`) requires an `X-CSRF-Token` request header. The token is the session-bound value embedded in every authenticated HTML page as `<meta name="csrf-token" content="...">`; browser-side fetch code should read it via `document.querySelector('meta[name=csrf-token]').content` and copy it into the header. Missing or mismatched token returns HTTP 403. See [security-posture.md](security-posture.md#cross-site-request-forgery-csrf).

| Method | Path | Description | Required role |
|---|---|---|---|
| GET | `/healthz` | Health check | none |
| GET | `/auth/login` | Begin OIDC flow; redirects to IdP | none |
| GET | `/auth/callback` | OIDC code exchange + session creation | none |
| GET / POST | `/auth/logout` | Clear session and (if available) redirect to IdP `end_session_endpoint` | none |
| GET | `/` | Dashboard | any authenticated |
| GET | `/targets` | Targets list | any authenticated |
| GET | `/targets/:id` | Target detail | any authenticated (members must own the target) |
| POST | `/api/targets` | Create a target | admin |
| PUT | `/api/targets/:id` | Update a target | admin |
| DELETE | `/api/targets/:id` | Delete a target | admin |
| GET | `/api/targets/:id/results` | List check results (JSON) | any authenticated (members must own the target) |
| GET | `/incidents` | Incidents list | any authenticated |
| POST | `/api/incidents/:id/resolve` | Manually resolve an incident | admin |
| GET | `/maintenance` | Maintenance windows list | any authenticated |
| POST | `/api/maintenance` | Create a maintenance window | admin |
| GET | `/channels` | Notification channels list | any authenticated (members see only their own) |
| GET | `/channels/:id` | Notification channel detail and edit page | any authenticated (members must own the channel) |
| POST | `/api/channels` | Create a notification channel | admin |
| PUT | `/api/channels/:id` | Update a notification channel | admin |
| DELETE | `/api/channels/:id` | Delete a notification channel | admin |
| POST | `/api/channels/:id/test` | Send a test notification to the channel and return the transport result. Rate-limited per channel (default 5/min, 30/hour); returns HTTP 429 with a `Retry-After` header when the limit is exceeded. | admin |
| POST | `/api/targets/:id/channels` | Attach a channel to a target | admin |
| DELETE | `/api/targets/:id/channels/:channel_id` | Detach a channel from a target | admin |
| GET | `/audit` | Audit log | admin |
| GET | `/api/admin/audit/verify` | Hash-chain integrity check over the entire audit log; returns JSON `{total_rows, legacy_rows, verified_rows, tampered_rows[]}`. Empty `tampered_rows` means the chain is intact. See [security-posture.md](security-posture.md#audit-log-tamper-detection). | admin |
| GET | `/me/security` | Personal account security page: current session, other active sessions for the same email, recent login history, and an audit-chain integrity check button (admin only). | any authenticated |
| POST | `/api/me/sessions/revoke-others` | Destroy every active session for the calling user *except* the one issuing the request. Returns JSON `{revoked: N}` where N is the number of sessions removed. CSRF token required. | any authenticated |
| GET | `/stats` | SLA / availability report (windowed: 24h/7d/30d/90d) | any authenticated (members see only their own targets) |
| GET | `/stats/:id` | Per-target SLA detail page with multi-window comparison and the windowed incident list | any authenticated (members must own the target) |
| GET | `/api/stats/sla?window=24h` | JSON pass-through of the same data, for scripting | any authenticated (members see only their own targets) |
| GET | `/api/stats/sla.csv?window=24h` | Per-target SLA report as CSV (UTF-8 with BOM, RFC 4180, `Content-Disposition: attachment`) | any authenticated (members see only their own targets) |
| GET | `/api/stats/incidents/:id.csv?window=24h` | Per-target window-scoped incident list as CSV | any authenticated (members must own the target) |
| GET | `/settings` | Settings + user management | admin |
| POST | `/api/settings/users` | Upsert a user | admin |
| GET | `/admin/migration` | Configuration migration page (export / import) | admin |
| GET | `/api/admin/migration/export?include_users=...` | Export configuration as JSON. `include_users=true` opts in to PII (emails). Returns with `Content-Disposition: attachment` so a browser fetch saves to disk. | admin |
| POST | `/api/admin/migration/import` | Import a previously-exported configuration. Body is `ImportRequest` (`payload`, `on_conflict: skip\|replace\|fail`, `apply: bool`). When `apply=false` (default) returns counts without writing. | admin |

## Core internal API

The Core has no public route and is only reachable from the Gateway via Service Binding (or from Cron). Every request must carry:

- `X-Gateway-Token`: the shared secret
- `X-Caller-UserId`, `X-Caller-Email`, `X-Caller-Name`, `X-Caller-Role`: the authenticated caller

The exception is `GET /users/lookup/:email`, which only requires `X-Gateway-Token` because it is invoked at authentication time before a Caller has been resolved.

| Method | Path | Description |
|---|---|---|
| GET | `/healthz` | Health check |
| GET | `/users/lookup/:email` | Look up a user by email; called by Gateway during authentication |
| GET | `/users` | List users |
| POST | `/users` | Upsert a user |
| GET | `/targets` | List targets (filtered by role) |
| GET | `/targets/summary` | Aggregate status counts |
| GET | `/targets/states` | Per-target state |
| GET | `/targets/:id` | Get one target |
| POST | `/targets` | Create a target |
| PUT | `/targets/:id` | Update a target |
| DELETE | `/targets/:id` | Delete a target |
| GET | `/targets/:id/state` | One target's state |
| GET | `/targets/:id/results?limit=N` | Recent check results for a target |
| GET | `/incidents?limit=N` | Recent incidents |
| POST | `/incidents/:id/resolve` | Resolve an incident |
| GET | `/maintenance` | List maintenance windows |
| POST | `/maintenance` | Create a maintenance window |
| GET | `/channels` | List notification channels (filtered by role) |
| POST | `/channels` | Create a notification channel |
| GET | `/channels/:id` | Get one notification channel |
| PUT | `/channels/:id` | Update a notification channel |
| DELETE | `/channels/:id` | Delete a notification channel (cascades to attachments) |
| POST | `/channels/:id/test` | Send a synthetic test notification through the channel; surfaces the transport error to the caller |
| GET | `/channels/:id/targets` | Reverse lookup: every target this channel is attached to. Used by the channel detail page to show the impact zone before mutating. |
| GET | `/targets/:id/channels` | List channels attached to a target |
| POST | `/targets/:id/channels` | Attach a channel to a target (idempotent upsert) |
| DELETE | `/targets/:id/channels/:channel_id` | Detach a channel from a target |
| GET | `/audit?limit=N` | Audit log entries |
| GET | `/audit/verify` | Walk the entire `audit_logs` table in `action_time ASC` order, recompute each row's `row_hash`, and return a `ChainVerification` JSON document classifying rows as legacy / verified / tampered. Pure read-only check. |
| GET | `/audit/login-history` | Return the calling user's recent `login` events from `audit_logs`, scoped by `actor_id`. Used by `/me/security` to render "Recent logins". |
| POST | `/audit/login` | Record a successful login event into `audit_logs`. Service-Binding token-authenticated only (no caller header — the just-logged-in user has no session readable by the browser yet). Body: `{user_id, user_email, ip_address}`. |
| GET | `/targets/:id/sla?window=24h` | Per-target SLA report. Window accepts `Nh` or `Nd` formats up to 90 days. |
| GET | `/targets/:id/sla/multi` | Per-target SLA reports for 24h, 7d, and 30d windows in a single round-trip. |
| GET | `/targets/:id/incidents?window=24h` | Window-scoped incident list for one target. |
| GET | `/stats/sla?window=24h` | Aggregate SLA report across every visible target (member-scoped) |
| GET | `/admin/migration/export?include_users=...` | Read every configuration table and return as `MigrationExport` JSON. Audit-logged. |
| POST | `/admin/migration/import` | Apply (or dry-run) a `MigrationExport` payload. Validates structurally, then writes under `Skip` / `Replace` / `Fail` policy. Audit-logged when `apply=true`. |

The Gateway's external surface and the Core's internal surface map almost 1:1; the Gateway primarily adds authentication, RBAC enforcement, and HTML rendering.
