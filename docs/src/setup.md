# Setup: first-time deployment to Cloudflare

This is the one-time setup walkthrough for taking a fresh checkout of Noye and deploying it to a Cloudflare account: provisioning D1 / KV / R2, setting up an OIDC client, registering secrets, seeding the initial admin user, and pushing both Workers.

**This is not the doc to read first.** If you just want to see Noye run on your machine without any Cloudflare account, the [README's Quick Start](../README.md#quick-start) covers `wrangler dev` in local mode and is enough for a feature tour.

**Related docs:**

- After your first successful deploy, see [deployment.md](deployment.md) for ongoing deploys, environments, rollouts, rollbacks, and the schema migration playbook.
- For secret-specific operations (rotation, compromise response), see [deployment-secrets.md](deployment-secrets.md).
- For moving an already-running deployment to a new Cloudflare account, see [migration.md](migration.md).

## Prerequisites

- Rust toolchain (rustc 1.85+ for Edition 2024)
- The `wasm32-unknown-unknown` target installed
- Node.js 18+
- Wrangler v4 (`npm install -g wrangler`)

## Step-by-step

### 1. Toolchain

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build
```

The repository ships a `.cargo/config.toml` that sets the rustc flag
`--cfg=getrandom_backend="wasm_js"` for the `wasm32-unknown-unknown` target.
This is required by `getrandom` 0.4 (pulled in by `wasm-smtp`'s default-on
`scram-sha-256` feature). No environment variable export is needed —
everyone building from the workspace root picks this up automatically.

### 2. Workspace verification

Make sure the codebase compiles before touching any cloud resources:

```bash
cargo check --workspace
```

### 3. Cloudflare resources

```bash
# D1 database (Core uses this)
wrangler d1 create noye_db
# → record the printed database_id in crates/core/wrangler.toml

# KV namespace (Gateway uses this for sessions and JWKS cache)
cd crates/gateway && wrangler kv namespace create CACHE_KV && cd ../..
# → record the printed id in crates/gateway/wrangler.toml

# R2 bucket (Core archives logs here)
wrangler r2 bucket create noye-logs
```

### 4. D1 schema migration

The migration files live in `sql/`. The Core's `wrangler.toml` references the directory via `migrations_dir = "../../sql"`. Wrangler applies them in alphanumeric order (`0001_initial.sql`, `0002_audit_hash_chain.sql`, …).

```bash
cd crates/core
wrangler d1 migrations apply noye_db
cd ../..
```

For an existing deployment that was provisioned before 0.18.0. applying `0002_audit_hash_chain.sql` adds the `prev_hash` / `row_hash` columns; pre-existing rows keep NULL hash values and the verifier classifies them as "legacy rows". The new chain begins with the next `INSERT`.

### 5. OIDC provider configuration

Pick an OpenID Connect-compliant provider and create an OAuth client there:

- Redirect URI: `https://<gateway-worker-domain>/auth/callback`
- Scopes: `openid`, `email`, `profile`
- Grant type: Authorization Code (PKCE-capable)

In `crates/gateway/wrangler.toml` `[vars]`, set:

- `OIDC_ISSUER_URL`
- `OIDC_CLIENT_ID`
- `OIDC_REDIRECT_URI`

The provider-specific issuer URL formats are listed in [oidc-providers.md](oidc-providers.md).

### 6. Shared secret between Gateway and Core

The two workers authenticate to each other with a shared secret. Generate one and register the same value on both workers:

```bash
SHARED_TOKEN=$(openssl rand -hex 32)
echo "Generated: $SHARED_TOKEN"   # keep this around for both wrangler secret put commands

cd crates/gateway
echo "$SHARED_TOKEN" | wrangler secret put GATEWAY_SHARED_TOKEN
cd ../..

cd crates/core
echo "$SHARED_TOKEN" | wrangler secret put GATEWAY_SHARED_TOKEN
cd ../..
```

Both workers fail-close when this secret is missing — without it, every Service Binding call is rejected with FORBIDDEN. (For local development, the shipped `wrangler.toml` ships with a well-known dev fallback in `[vars]`; production must override it as above.)

### 7. OIDC client secret

The OIDC client secret is registered only on the Gateway:

```bash
cd crates/gateway
wrangler secret put OIDC_CLIENT_SECRET
# (paste the client secret when prompted)
cd ../..
```

### 8. Switch from development to production mode

Both workers' `wrangler.toml` files ship with `NOYE_ENV = "development"` for `wrangler dev` convenience. Production deploys must change this:

```bash
# Edit each wrangler.toml in crates/gateway and crates/core, change:
#    NOYE_ENV = "development"
# to:
#    NOYE_ENV = "production"
```

While you're there, **remove** the two `[vars]` lines that hold dev-only fallbacks:

```toml
# Remove from crates/gateway/wrangler.toml:
OIDC_CLIENT_SECRET = "dev-idp-does-not-verify-this"
GATEWAY_SHARED_TOKEN = "noye-local-dev-shared-token"

# Remove from crates/core/wrangler.toml:
GATEWAY_SHARED_TOKEN = "noye-local-dev-shared-token"
```

If you skip this step, the gateway will refuse to serve any request and return an error message naming the offending variable. Both workers run an independent self-check on every request — see [security-posture.md](security-posture.md#leaked-dev-fallback-detection).

### 9. Initial admin user

Noye does not auto-provision users. Insert the first admin manually so they can later add other users from the UI:

```bash
cd crates/core
wrangler d1 execute noye_db --command \
  "INSERT INTO users (id, email, name, role) VALUES (
     'admin-001', 'admin@example.com', 'Admin', 'admin'
   )"
cd ../..
```

The email value must match the email claim issued by your OIDC provider for that account.

### 10. Deploy

Order matters: the Gateway's Service Binding to the Core requires the Core to exist already.

```bash
cd crates/core    && wrangler deploy && cd ../..
cd crates/gateway && wrangler deploy && cd ../..
```

## Local development

Service Bindings work in local development. Run each worker in its own terminal:

```bash
# terminal 1
cd crates/core && wrangler dev

# terminal 2
cd crates/gateway && wrangler dev
```

The Gateway's `wrangler dev` automatically wires the Service Binding to the locally-running Core.

## Smoke test

After deployment, hit the health endpoints:

```bash
curl https://<gateway-domain>/healthz   # → 200 "ok"
```

The Core's `/healthz` is intentionally unreachable from the outside; you can only reach it through the Gateway.
