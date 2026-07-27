# Deployment: troubleshooting

This document maps observed symptoms to likely causes and concrete fixes. Most production failures of a Noye deployment fit into one of the patterns below.

## Diagnostic starting points

Before drilling into specifics, two commands give you the fastest signal:

```bash
# Public health check
curl -i https://<gateway-domain>/healthz
# Expected: HTTP/2 200, body "ok"

# Live tail of Gateway logs (run in another terminal during reproduction)
cd crates/gateway && wrangler tail
```

`wrangler tail` streams every request and every `console_log!` / `console_error!` from the running worker. You can also tail the Core (`cd crates/core && wrangler tail`) — this is essential for diagnosing Service Binding failures because the Core is otherwise invisible.

## Symptoms

### Gateway returns 500 on every page

**Likely causes:**

1. **Missing or wrong `GATEWAY_SHARED_TOKEN`.** Every authenticated page does an `extract_caller` call that goes to the Core. The Core fails-closed when the token is missing — see "configuration error: GATEWAY_SHARED_TOKEN ..." below.
2. **Core is not deployed.** The Service Binding target does not exist; every call fails immediately.
3. **`OIDC_CLIENT_ID` or `OIDC_ISSUER_URL` mismatched.** The Discovery fetch fails; the Gateway cannot complete an OIDC handshake or even initiate one.
4. **Leaked dev fallback in production.** `NOYE_ENV` is not `"development"`, but the `[vars]` section still holds the dev-fallback `OIDC_CLIENT_SECRET = "dev-idp-does-not-verify-this"` or `GATEWAY_SHARED_TOKEN = "noye-local-dev-shared-token"`. The gateway's startup self-check refuses to serve any request and logs `configuration error: <NAME> has its development-fallback value in production.`

**Diagnostic:**

```bash
# Confirm both workers are deployed
wrangler deployments list   # in each crates/<n> directory

# Check secrets exist (values are hidden, presence is shown)
cd crates/gateway && wrangler secret list && cd ../..
cd crates/core    && wrangler secret list && cd ../..

# Verify OIDC discovery URL is reachable from your machine
curl "$OIDC_ISSUER_URL/.well-known/openid-configuration"

# Tail logs from both workers; the dev-fallback check logs a clear message
wrangler tail   # in each crates/<n> directory
```

If `wrangler tail` shows `configuration error: <NAME> has its development-fallback value in production`, edit the corresponding `wrangler.toml`, remove the `[vars]` line for that variable, and re-deploy after `wrangler secret put <NAME>`.

If `wrangler tail` on the Core shows `FORBIDDEN: invalid gateway token`, the secrets are mismatched between the two workers. Re-register them with the [rotation procedure](deployment-secrets.md#gateway_shared_token-brief-planned-outage).

If `wrangler tail` on the Core shows `FORBIDDEN: GATEWAY_SHARED_TOKEN not configured`, the secret was never registered there. Run `wrangler secret put GATEWAY_SHARED_TOKEN` in `crates/core` with the same value as on the Gateway.

### "Service binding to CORE failed" or 502 on authenticated pages

**Likely cause:** Core has been deleted, renamed, or moved to a different environment from the one the Gateway expects.

**Diagnostic:**

The `[[services]]` block in `crates/gateway/wrangler.toml` declares the binding. If Core's worker name has changed (perhaps because an environment was added), the Gateway needs to be redeployed against the new name.

```bash
# Confirm both workers are in the same environment
wrangler deployments list --env production   # in crates/gateway
wrangler deployments list --env production   # in crates/core
```

If they live in different environments, a Service Binding will not bridge them — it must point to a worker in the same account and environment.

### Login redirect loops between Gateway and IdP

**Symptom:** the user clicks login, gets redirected to the IdP, authenticates, gets redirected back to the Gateway, and immediately gets redirected to the IdP again.

**Likely causes:**

1. **`OIDC_REDIRECT_URI` does not exactly match what's registered at the IdP.** Most IdPs require an exact-string match including the scheme, host, port, and path. A trailing slash or a `http`/`https` mix-up triggers this.
2. **Cookies are being dropped.** The session cookie is `Secure` (won't survive over HTTP) and `SameSite=Lax`. If the Gateway is fronted by a proxy that strips cookies or rewrites the host header, the cookie write succeeds but the read fails on the next request.
3. **System clock drift on the user's machine.** ID Token validation has 60 seconds of leeway; clock skews larger than that cause the token to be rejected as expired and the user is sent back through the flow.

**Diagnostic:**

In `wrangler tail` for the Gateway, you should see one of:

- `OIDC error: ...` — the IdP rejected the request (usually redirect URI mismatch). The error_description body has the specifics.
- `Missing state parameter` / `state mismatch` — KV state has expired between login redirect and callback (the PendingLogin entry has a 600-second TTL; redirects taking longer than 10 minutes will fail).
- `ID Token has no email claim` — the OIDC scope `email` was not granted by the IdP. Check the OAuth client's scope configuration.

### Cron is not running

**Symptom:** No new rows in `check_results`. Targets are listed but `last_checked_at` does not advance.

**Likely causes:**

1. **`workers_dev` is true on Core.** When `workers_dev = true`, Cron Triggers do not fire on the workers.dev subdomain; they only fire when the worker has a custom route or is in production mode. For Noye's intentional design, Core has `workers_dev = false` and *no* route — but Cron does still fire. Verify in `crates/core/wrangler.toml`.
2. **Cron trigger configuration is missing.** Check `crates/core/wrangler.toml` for the `[triggers]` block. It should contain `crons = ["* * * * *"]`.
3. **Worker is failing to deploy.** A deploy that fails silently (rare, but possible if the wasm build artifact is malformed) leaves the previous version running. `wrangler deployments list` shows the current active version.

**Diagnostic:**

```bash
# Tail the Core (Cron logs appear here even though no HTTP request triggered them)
cd crates/core && wrangler tail
# Wait up to 60 seconds. You should see a "scheduled" event approximately once per minute.
```

If you see no scheduled events at all, the trigger is misconfigured. If you see them but no checks run, the issue is upstream of Cron — usually `D1_BINDING` failure (Core can't read targets) or no targets are due (`next_check_at` is in the future for everything).

### "Database is locked" / D1 errors

**Likely cause:** D1 has weaker concurrency guarantees than a full SQLite. In particular, two writes to the same row from concurrent invocations can result in `SQLITE_BUSY` even though the underlying database is technically multi-master.

**Mitigations already in the codebase:**

- The `update_after_check` function in `db::states` does a single read-then-write per target, with no fan-out.
- The Cron handler processes targets sequentially, not in parallel, so concurrent writes within one Cron run are not possible.

**If you still see D1 errors:**

The most common cause is a long-running deployment leaving two versions of the Core active during propagation. This is a transient condition; wait 30 seconds and try again. If it persists, check `wrangler deployments list` for stuck or partially-completed deployments.

### KV reads return stale data

Cloudflare KV is eventually consistent globally. A `put` propagates to all edge locations within ~60 seconds. Two situations where this matters:

1. **Session cookie immediately after login.** The session is written to KV during the OIDC callback, then the user is redirected back to a page that reads the session. If the read hits a different edge location than the write, the session may not be visible yet, and the user gets bounced back to login.

   In practice this is rare because the user's followup request usually hits the same edge node, but it does happen. The user's retry one second later always works.

2. **JWKS cache.** The OIDC provider's signing keys are cached in KV with a 1-hour TTL. If the IdP rotates keys, there can be up to an hour during which Noye still trusts the old key. The first JWT it cannot verify forces a JWKS refresh, so the impact is limited to a single rejected login and an automatic retry.

For ongoing monitoring of KV health, the dashboard's KV namespace metrics show read/write success rates per edge region.

### Notifications not arriving

**Likely causes (in order of frequency):**

1. **Channel is disabled.** Check the channel's `is_enabled` column or use the `/channels` UI.
2. **No channel attached to the target.** The `target_notifications` table is empty for that target.
3. **`on_down` / `on_up` flag is false.** A channel can be attached with both flags off — in which case it is a no-op.
4. **State did not actually transition.** The notify dispatch only fires on transitions, not on every check. If the target is bouncing between up and down without crossing the threshold (default 3), no notification is sent.
5. **The endpoint URL is bad.** The Cron handler logs `Failed to send DOWN notification via <type>: <error>` to `console_error!`. Tail the Core to see these messages. Use the [test-send feature](api.md#core-internal-api) to verify the endpoint without waiting for a real outage.

**Diagnostic:**

The fastest path is the test-send button on `/channels`. It uses the same dispatch code as Cron, so a successful test send confirms that:

- The channel record is correctly configured
- The endpoint is reachable from the Cloudflare edge
- The endpoint accepts the payload format Noye sends

If the test send works but real outage notifications don't arrive, the problem is in the state-transition or attachment layer rather than the dispatch layer.

### Email test send fails with "smtp send failed: ..."

The error message comes through verbatim from `wasm-smtp`'s `Display` impl. Common shapes:

- **`smtp transport error: ...`** — TLS handshake or socket-level failure. Verify with `openssl s_client -connect host:465` (Implicit TLS) or `openssl s_client -starttls smtp -connect host:587` (STARTTLS).
- **`auth rejected by server: ...`** — relay refused the credentials. Check `EMAIL_SMTP_USERNAME` and the `EMAIL_SMTP_PASSWORD` secret. For SES, the SMTP credentials are different from the IAM access key — they must be generated specifically as "SMTP credentials" in the SES console.
- **`auth scram-sha-256: server-nonce mismatch`** (or other SCRAM-specific message) — only occurs against a relay that advertises `SCRAM-SHA-256`. Indicates the relay's SCRAM implementation deviates from RFC 7677 or the client nonce was tampered with mid-flight. Falling back to `PLAIN` is not currently exposed via configuration; if a specific relay reproduces this, file an upstream `wasm-smtp` issue with a packet capture.
- **`unexpected response code 421/450/451`** — the server refused the message at the protocol level. Read the surrounding text — it usually contains the reason (rate limit, IP reputation, missing SPF, etc.).

### "Rate limit exceeded" on test send (HTTP 429)

The per-channel rate limit on test-send is 5/minute and 30/hour by default. The `Retry-After` response header tells you how long to wait. If you frequently hit the limit during legitimate troubleshooting, raise the limits in `crates/gateway/wrangler.toml`:

```toml
[vars]
TEST_SEND_LIMIT_PER_MIN = "30"
TEST_SEND_LIMIT_PER_HOUR = "300"
```

then redeploy the Gateway. The new limits take effect immediately.

### "Too many login attempts" (HTTP 429 on `/auth/login`)

The per-IP login rate limit is 10/minute and 50/hour by default. Hitting this from a single IP is unusual for normal usage — a real user logs in once or twice a day at most. Likely causes:

- **Shared NAT egress.** Many users behind the same public IP (corporate proxy, mobile carrier NAT) can collectively exceed the limit. If this is the expected pattern, raise the limits in `crates/gateway/wrangler.toml`:

  ```toml
  [vars]
  LOGIN_RATE_LIMIT_PER_MIN = "60"
  LOGIN_RATE_LIMIT_PER_HOUR = "300"
  ```

  Then redeploy the Gateway.

- **Repeated OIDC callback failure causing the user to retry the login flow.** Check for an underlying error (mismatched `OIDC_REDIRECT_URI`, expired client secret, IdP outage). The 429 will clear once the underlying error is fixed and the window expires.

- **Active scanning or DoS attempt.** Cross-check the source IP in `wrangler tail` logs. Block at the Cloudflare WAF level if the same IP appears repeatedly without legitimate session establishment.

The `Retry-After` response header tells the client how long to wait. Resetting an individual IP is not currently supported via UI; the buckets expire automatically (75-minute TTL on the hour bucket).

### "CSRF token missing / malformed / mismatch" (HTTP 403)

State-changing requests (`POST` / `PUT` / `DELETE`) require an `X-CSRF-Token` header that matches the session-bound token. The token is embedded in every authenticated HTML page as `<meta name="csrf-token" content="...">`. Likely causes:

- **Browser without JavaScript / browser tab cached an older HTML version**. The page's `<meta>` predates a session change; reload the page (Ctrl-Shift-R) to fetch the current token.
- **Stale tab open across a logout / re-login**. The browser still has the previous session's token in the `<meta>`; reload the page after re-logging-in.
- **External script calling Gateway directly**. Scripts driving the Gateway from outside the browser (curl, Postman, custom integrations) need to first `GET /` to read the current token from the `<meta>` tag, then include it in subsequent requests. For purely server-to-server automation, prefer talking to the Core via Service Binding instead — the `/api/admin/migration/import` route exists for both human and machine clients, but the human path is the supported one for state changes.
- **A request landed against a session that predates the 0.19.0 deploy** (very rare — the legacy-session opt-out logs a `[csrf]` console warning and allows the request once). If you see persistent 403s on legacy sessions, the user should log out and back in to receive a fresh token.

The exact reason is in the response body: "CSRF token missing", "CSRF token malformed", "CSRF check requires an active session", or "CSRF token mismatch". Tail logs surface a `[csrf]` line for the legacy-session allow path.

### `wrangler tail` shows nothing but the worker is being hit

**Likely cause:** `wrangler tail` connects to a specific deployment. If a new deployment landed after you started tailing, your tail is connected to the old (now-inactive) version.

**Fix:** restart `wrangler tail`. The fresh connection picks up the active deployment.

## When in doubt: re-deploy

For transient issues that don't match any specific symptom above, redeploying both workers in the standard order (Core then Gateway) often clears them. Cloudflare Workers deployments are atomic and cheap; the bar for "try a redeploy" is low.

```bash
cd crates/core    && wrangler deploy && cd ../..
cd crates/gateway && wrangler deploy && cd ../..
```

If a redeploy makes things worse, the [rollback procedure](deployment.md#rollback) gets you back to the last known-good state in seconds.
