# Architecture

Noye is a two-worker system on Cloudflare Workers, separated cleanly into a publicly-reachable Gateway and an internally-reachable Core.

## Layer responsibilities

| Layer | Responsibilities | Reachable from the Internet | Bindings it owns |
|---|---|---|---|
| **Gateway** | OIDC authentication; session management; UI server-side rendering; forwarding requests to the Core | Yes (HTTPS) | KV (sessions, OIDC state, JWKS cache); Service Binding to Core |
| **Core** | D1 access; monitoring engine; notification dispatch; Cron-driven scheduling | No | D1; R2; Cron Triggers |

## Security: defense in depth

The architecture stacks several independent layers of protection so that a single misconfiguration cannot expose data or compromise the system.

1. **Route isolation.** The Core sets `workers_dev = false` and has no custom route. Cloudflare's routing layer alone makes the Core unreachable on the public Internet.
2. **Service Binding only.** The only ingress points into the Core are (a) Service Binding calls from the Gateway and (b) Cron Triggers. There is no third path.
3. **Shared secret.** Each Service Binding call carries an `X-Gateway-Token` header. The token is registered as a Wrangler secret on both workers; the Core rejects any call where the header does not match.
4. **Data isolation.** The Gateway has no D1 binding at all. Even if Gateway code were compromised, it could not read or write the database directly — every data operation must go through the Core's REST API.
5. **OIDC authentication.** Web-standards Authorization Code + PKCE (S256) + state + nonce. ID Tokens are verified against JWKS via the Web Crypto API.
6. **Guest rejection.** Even after a successful OIDC sign-in, a user that is not pre-registered in the D1 `users` table is rejected with HTTP 403. There is no implicit user creation.
7. **Rate limiting on outbound test sends.** The "send test notification" action is rate-limited per channel via `crates/gateway/src/rate_limit.rs` (fixed-window KV counters at minute and hour granularity, defaults 5/min and 30/hour). The check runs on the Gateway *before* the Service Binding hop, so abusive bursts cannot reach either the Core or the upstream notification endpoint.

## Caller propagation

When a request crosses from Gateway to Core, the caller's identity is encoded into HTTP headers:

| Header | Purpose |
|---|---|
| `X-Gateway-Token` | Shared secret; verified by the Core on every call |
| `X-Caller-UserId` | Caller's user ID (D1 row ID) |
| `X-Caller-Email` | Caller's email (already verified by OIDC) |
| `X-Caller-Name` | Caller's display name |
| `X-Caller-Role` | Caller's role: `admin` or `member` |

These header names are defined once in `noye-shared::header` and reused by both workers, so the contract is a single source of truth.

## Data flow

A typical authenticated UI request:

1. Browser issues `GET /targets`.
2. Gateway's `authenticate()` reads the session cookie, looks the user up via Core's `/users/lookup/:email`, and constructs a `Caller`.
3. Gateway's route handler calls `core_client::list_targets(env, &caller)`.
4. The `core_client` builds a Service Binding HTTP request to the Core's `/targets` endpoint, attaching `X-Gateway-Token` and `X-Caller-*` headers.
5. The Core's `api::targets::list` validates the gateway token, reconstructs the `Caller` from the headers, queries D1, and returns JSON.
6. Gateway renders the response into HTML and returns it to the browser.

A monitoring tick:

1. Cron Trigger fires once per minute on the Core.
2. `monitor::engine::run_scheduled_checks` queries D1 for targets whose `next_check_at` has been reached.
3. For each target, the protocol-specific checker runs (HTTP/HTTPS/TCP/SMTP/TLS).
4. Results land in `check_results`. State transitions (down/recovery) are applied to `target_states`.
5. On state transitions, an audit log entry is created (actor `system`, recorded as a snapshot rather than a live reference to a user row — see `security-posture.md` § Audit logging) and notifications are dispatched, suppressed if a maintenance window covers the target.

## Project layout

The workspace uses the modern flat module convention (`<module>.rs` next to `<module>/`) — no `mod.rs`.

```
noye/
├── Cargo.toml                          # workspace root
├── shared/
│   └── src/lib.rs                      # cross-boundary types + header names
└── workers/
    ├── gateway/
    │   └── src/
    │       ├── lib.rs                  # routes + authenticate() wrapper
    │       ├── auth.rs                 # extract_caller via core_client
    │       ├── auth/
    │       │   ├── oidc.rs             # Discovery, Auth Request, Token Exchange
    │       │   ├── jwt.rs              # JWT parse + claim validation
    │       │   ├── jwks.rs             # JWKS fetch + KV cache
    │       │   ├── crypto.rs           # Web Crypto wrapper
    │       │   ├── session.rs          # KV-backed session store
    │       │   ├── cookie.rs           # cookie parser/builder
    │       │   └── rbac.rs             # role-based access control
    │       ├── core_client.rs          # Service Binding RPC to Core
    │       └── ui.rs + ui/             # SSR HTML renderers (ABDD-compliant)
    └── core/
        └── src/
            ├── lib.rs                  # internal-API fetch + scheduled (Cron)
            ├── api.rs                  # auth middleware
            ├── api/                    # internal REST handlers
            ├── db.rs + db/             # D1 CRUD
            ├── monitor.rs + monitor/   # protocol checkers + engine
            └── notify.rs + notify/     # webhook / Slack dispatch
```
