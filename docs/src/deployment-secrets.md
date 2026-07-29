# Deployment: secrets

This document is the authoritative inventory of every secret Noye depends on, what each one protects, and how to rotate it without downtime.

## Secret inventory

| Worker | Secret | Mandatory? | What breaks if it's missing or wrong |
|---|---|---|---|
| Gateway | `OIDC_CLIENT_SECRET` | Yes (production) | Token-exchange step of the OIDC flow fails; users land on a 500 page after IdP redirect. If the dev fallback `"dev-idp-does-not-verify-this"` is still present, every request is rejected at the edge, **in every environment, not only production** — see [security-posture.md](security-posture.md#leaked-dev-fallback-detection). |
| Gateway | `GATEWAY_SHARED_TOKEN` | Yes (production) | Every Service Binding call to Core gets rejected with 403 once Core has a value too |
| Core | `GATEWAY_SHARED_TOKEN` | Yes (production) | **Fail-closed** since 0.14.0: when missing, every Service Binding request is rejected with FORBIDDEN. (Earlier versions silently fell back to permissive mode — that hole is now closed.) Same fail-closed treatment applies to a leaked dev-fallback value, in every environment — Core does not read `NOYE_ENV` at all. |
| Gateway | `TURNSTILE_SECRET_KEY` | Conditional | Required only when `TURNSTILE_SITE_KEY` is non-empty; missing it produces a 500 on Turnstile-protected forms |
| Core | `EMAIL_SMTP_PASSWORD` | Conditional | Required only when `EMAIL_SMTP_HOST` is non-empty; missing it makes every email channel fail with a clear "EMAIL_SMTP_PASSWORD secret is not registered" error rather than silently dropping mail |

The list is short on purpose. Avoid introducing additional secrets without checking whether an existing one can be reused or whether the value belongs in `[vars]` (public) instead.

## Generating fresh values

`GATEWAY_SHARED_TOKEN` should be cryptographically random and at least 32 bytes worth of entropy. Generate with:

```bash
openssl rand -hex 32
```

`OIDC_CLIENT_SECRET` is issued by your IdP — copy it from the IdP admin console at the moment of OAuth client creation. Most IdPs only show the secret once; if you miss it, you have to re-issue.

`TURNSTILE_SECRET_KEY` is issued by Cloudflare; retrieve it from the Turnstile dashboard alongside the matching site key.

## Registration

Secrets are not stored in `wrangler.toml`. Use `wrangler secret put`, which writes them to the Cloudflare-managed secret store:

```bash
# Gateway
cd crates/gateway
echo "$VALUE" | wrangler secret put OIDC_CLIENT_SECRET
echo "$VALUE" | wrangler secret put GATEWAY_SHARED_TOKEN
cd ../..

# Core (only one secret needed)
cd crates/core
echo "$VALUE" | wrangler secret put GATEWAY_SHARED_TOKEN
cd ../..
```

For multi-environment setups (`--env staging`, `--env production`, etc.), every secret must be set per-environment. There is no environment inheritance.

To list secrets currently registered (without revealing values):

```bash
cd crates/gateway && wrangler secret list && cd ../..
cd crates/core    && wrangler secret list && cd ../..
```

To delete:

```bash
wrangler secret delete <NAME>
```

## Rotation

### `GATEWAY_SHARED_TOKEN` (brief planned outage)

Because both workers must agree on the token at every point in time, rotating it requires a brief outage. The simple coordinated rollout:

1. Generate the new value: `NEW=$(openssl rand -hex 32)`
2. Update the Gateway's secret: `echo "$NEW" | wrangler secret put GATEWAY_SHARED_TOKEN` (in `crates/gateway`).
3. Update the Core's secret to the same value: `echo "$NEW" | wrangler secret put GATEWAY_SHARED_TOKEN` (in `crates/core`).
4. Both workers are now strict again with the new token.

In the window between steps 2 and 3, calls from Gateway to Core are rejected with FORBIDDEN. Plan rotation accordingly — the window is typically a few seconds.

(In versions ≤ 0.13.0. this rotation could be done with zero downtime by temporarily unsetting `GATEWAY_SHARED_TOKEN` on the Core, which then accepted any caller. 0.14.0 closed that fail-open hole. The trade-off is intentional: a couple of seconds of 403s is preferable to leaving a permissive bypass available for misconfigured deploys.)

A more elegant approach (not yet implemented) would be for the Core to accept an array of valid tokens during a configured grace window. Worth doing if rotation frequency increases.

### `OIDC_CLIENT_SECRET`

Most IdPs let you issue a second client secret while the old one is still valid:

1. In the IdP admin console, create a second secret for the same OAuth client. The old one keeps working.
2. Update the Gateway's `OIDC_CLIENT_SECRET` to the new value.
3. Verify a fresh login still works end-to-end.
4. Revoke the old secret in the IdP admin console.

If your IdP does not support two simultaneous secrets, you have to take a brief outage on the OIDC flow (~30 seconds for the secret to propagate). Existing sessions are unaffected — the secret only matters for new logins and refreshes.

### `TURNSTILE_SECRET_KEY`

Cloudflare lets you rotate the secret key without invalidating the matching site key. Follow the dashboard's rotation flow, then update the Gateway secret. There is no overlap window required because the site key (public) is what binds the widget to the verification endpoint, not the secret.

### `EMAIL_SMTP_PASSWORD`

Most SMTP relay providers (SES, SendGrid, Mailgun, Resend) issue API keys or SMTP credentials that can be created in pairs and rotated without downtime:

1. Issue a new credential at the provider while the old one is still active.
2. Update the Core's `EMAIL_SMTP_PASSWORD` to the new value: `cd crates/core && wrangler secret put EMAIL_SMTP_PASSWORD`.
3. Trigger a test send via the `/channels` UI to verify the new credential works end-to-end.
4. Revoke the old credential at the provider.

If your relay does not support paired credentials, expect a brief gap where Cron-driven email notifications fail with `smtp send failed` until the new password propagates (~30 seconds).

## Compromise scenarios

### `GATEWAY_SHARED_TOKEN` is leaked

An attacker with this token can call the Core's Service Binding endpoint *if and only if* they also have a Service Binding to the Core deployed in their own Cloudflare account. Service Bindings are scoped to a single account, so the attacker would need to have already compromised your Cloudflare account. The realistic threat is therefore an internal-misuse scenario (a developer account with too much access, a leaked CI secret).

Response: rotate the token immediately using the procedure above. The Core's audit log records `actor_id = 'system'` for un-authenticated calls and the Caller email for authenticated ones, so post-incident review can identify what was touched.

### `OIDC_CLIENT_SECRET` is leaked

An attacker with this secret can complete the OIDC token exchange impersonating Noye. The practical risk depends on the IdP — most modern IdPs require the redirect URI to match the registered value, which limits the damage. But you should still treat this as a P1.

Response: rotate at the IdP first (issue a new secret, update Noye, revoke old). Force-invalidate all sessions by rotating `SESSION_COOKIE_NAME` (in `wrangler.toml` `[vars]`); existing sessions stop working and users have to re-authenticate.

### A user account is compromised

An attacker with valid IdP credentials for a registered Noye user gets that user's role. Mitigations:

- **Audit log review.** Every mutation is recorded in `audit_logs` with the actor's email, IP address, and timestamp. Use the `/audit` page (admin-only) to walk back the activity.
- **Disable the user.** Set `is_active = 0` on their `users` row via `wrangler d1 execute noye_db --command "UPDATE users SET is_active = 0 WHERE email = '<email>'"`. The next request from that user will hit the `FORBIDDEN: user inactive` path.
- **Rotate sessions.** Same as above — change `SESSION_COOKIE_NAME` to invalidate every active session.

## Storage

`wrangler secret put` stores values in Cloudflare's secret store, which is encrypted at rest and only decrypted inside the runtime that needs them. Secrets are never accessible from `wrangler.toml` reads, the dashboard, or `wrangler tail`. The only way to retrieve a value is to write code that reads it via `env.secret(...)`, deploy that code, and read its output — meaning a successful exfiltration requires deploy access.

This is why deploy access (the `Cloudflare API token` for `wrangler deploy`) is itself a sensitive credential. Treat the API token with the same care as the secrets it can be used to read.
