# Local OIDC stub (`noye-dev-idp`)

A small, local-only OIDC Identity Provider that lets you exercise the Noye Gateway's full sign-in flow without setting up an external IdP (Google, Okta, etc.). Use it for development and integration tests; do not deploy it.

## What it does

`noye-dev-idp` implements the minimum subset of OIDC Core 1.0 needed for Noye's gateway to complete a login round-trip:

| Endpoint | Method | Purpose |
|---|---|---|
| `/.well-known/openid-configuration` | GET | OIDC Discovery document |
| `/jwks` | GET | JSON Web Key Set (the public side of the signing key) |
| `/authorize` | GET | Stash `state`/`nonce`/`code_challenge`, redirect to the gateway with `code` |
| `/token` | POST | Verify PKCE, mint a signed RS256 ID Token |
| `/healthz` | GET | Health check (returns `{"ok": true}`) |

A fresh RSA-2048 keypair is generated on every start. Sign-ins are accepted only for one hard-coded user — `admin@local.test`, `sub = local-admin-1` — which lets the gateway map the OIDC identity to a row in your local Noye `users` table.

## Running it

```bash
cargo run -p noye-dev-idp
```

Default address: `http://localhost:5556`.

Environment variables:

| Variable | Default | Purpose |
|---|---|---|
| `DEV_IDP_PORT` | `5556` | Listening port |
| `DEV_IDP_USER_EMAIL` | `admin@local.test` | Email claim emitted in the ID Token |
| `DEV_IDP_USER_NAME` | `Local Admin` | Name claim emitted in the ID Token |

The default email matches what `crates/gateway/wrangler.toml` and `noye admin create` use, so the three tools agree out of the box.

## Wiring it up

The repo's default `crates/gateway/wrangler.toml` already points at this stub:

```toml
OIDC_ISSUER_URL    = "http://localhost:5556"
OIDC_CLIENT_ID     = "noye-local-client"
OIDC_REDIRECT_URI  = "http://localhost:8787/auth/callback"
OIDC_CLIENT_SECRET = "dev-idp-does-not-verify-this"   # dev fallback in [vars]
```

`OIDC_CLIENT_SECRET` is not actually validated by `noye-dev-idp` (which is the entire point of "stub"). It must still be present in the gateway's environment because the gateway sends it on every `/token` request — but any non-empty value works.

Before the gateway sees you as a recognized user, run:

```bash
cargo run -p noye -- admin create \
  --email admin@local.test --name "Local Admin"
```

This inserts a matching row into the gateway's local D1 `users` table.

## Limitations (intentional)

- **Single user.** No registration UI, no per-request user override. To test multi-user behavior, use a real IdP like Google, Auth0, or Keycloak — see [oidc-providers.md](oidc-providers.md).
- **Keys regenerate on restart.** Sessions issued by the gateway during a previous dev-idp run will still be valid (sessions live in KV, not in this stub), but ID Token verification by the gateway requires the JWKS to match. The gateway's JWKS cache will refresh automatically after its TTL.
- **No `client_secret` enforcement.** Anyone reaching the `/token` endpoint with a valid `code` and PKCE proof gets an ID Token. This is fine on `localhost`; do not expose this binary to a network.
- **No refresh tokens.** OIDC refresh flow is not implemented; the gateway works without it.
- **Codes expire after 60 seconds.** They are also single-use, which matches the OIDC spec.

## What it is **not** for

- Production deploys. Use a real IdP. See [oidc-providers.md](oidc-providers.md) for configuration tables.
- CI integration tests against a deployed Noye. Same reason — production should use the real auth path.
- Loading testing the OIDC flow. RSA-2048 signing has measurable cost; use a real IdP.

## Behind the scenes

Source tree at `crates/dev-idp/`:

```
src/
├── main.rs         # tokio runtime, hyper server, route dispatch entry
├── handlers.rs     # /authorize, /token, /jwks, /.well-known/openid-configuration
├── jwt.rs          # RS256 ID Token construction (header, payload, signature)
├── keys.rs         # RSA-2048 keypair generation and JWK encoding
└── state.rs        # in-memory store of pending authorization codes
```

Total: ~600 lines of Rust including doc comments and tests. Dependencies: `hyper` 1.x, `rsa` 0.9, `tokio`, `serde_json`, `chrono`, `uuid`. No external service.
