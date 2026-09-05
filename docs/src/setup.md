# Setup: first-time deployment to Cloudflare

This is the one-time setup walkthrough for taking a fresh checkout of Noye and deploying it to a Cloudflare account. `noye provision` (subject 37) does the mechanical part — creating D1/KV/R2, applying migrations, registering secrets, seeding the initial admin user, and deploying both Workers — so this page is now short: toolchain, `wrangler login`, register an OAuth client, run `provision` twice (plan, then apply), smoke test.

**This is not the doc to read first.** If you just want to see Noye run on your machine without any Cloudflare account, the [README's Quick Start](../README.md#quick-start) covers `wrangler dev` in local mode and is enough for a feature tour.

**Related docs:**

- After your first successful deploy, see [deployment.md](deployment.md) for ongoing deploys, environments, rollouts, rollbacks, and the schema migration playbook.
- For secret-specific operations (rotation, compromise response), see [deployment-secrets.md](deployment-secrets.md).
- For moving an already-running deployment to a new Cloudflare account, see [migration.md](migration.md).
- For what `provision` does step by step, and how to do it by hand instead, see the [appendix](#appendix-the-manual-walkthrough) below.

## Prerequisites

- Rust toolchain (rustc 1.85+ for Edition 2024)
- The `wasm32-unknown-unknown` target installed
- Node.js 18+
- Wrangler v4 (`npm install -g wrangler`)

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build
cargo check --workspace   # make sure the codebase compiles before touching any cloud resources
```

The repository ships a `.cargo/config.toml` that sets the rustc flag `--cfg=getrandom_backend="wasm_js"` for the `wasm32-unknown-unknown` target. This is required by `getrandom` 0.4 (pulled in by `wasm-smtp`'s default-on `scram-sha-256` feature). No environment variable export is needed — everyone building from the workspace root picks this up automatically.

## What `provision` cannot do for you

Three things need a human regardless of tooling — `provision` prints this same list before doing anything else, so read it once here and again when it prints:

1. **`wrangler login`** — authenticate this machine to your Cloudflare account.
2. **Register an OAuth client** at your identity provider (issuer URL, client id, client secret). `provision` prints the Gateway's redirect URI *before* asking for these, since the ordering problem — the doc used to ask for the redirect URI before the URL that needs it existed — is exactly what made this a two-pass job before.
3. **Have the admin's email ready** — the first user, matching the email claim your provider will issue for that account.

Provider-specific issuer URL formats are listed in [oidc-providers.md](oidc-providers.md).

## Provision

```bash
noye provision --plan
```

Prints every action `provision` would take — every derived name, every file it would write — without changing anything. No `wrangler` subprocess runs, no file is written. Read it before running the real thing.

Deploying a second environment (staging, alongside production) on the same Cloudflare account:

```bash
noye provision --plan --env-name staging
```

`--env-name` (default `noye`) derives every resource and worker name from one input, so two deployments never collide: `noye-core`/`noye-staging-core`, `noye_db`/`noye_staging_db`, and so on ([G-46](decision-log.md)).

Once the plan looks right:

```bash
noye provision --apply
```

This creates D1/KV/R2, applies every migration in `sql/`, generates the Gateway↔Core shared secret and registers it on both workers (never printed — piped straight to `wrangler secret put`), registers the OIDC client secret, creates the initial admin user (via `noye admin create`, not raw SQL), and deploys Core before Gateway. You'll be prompted for the OIDC issuer URL, client id, client secret, and admin email/name if you didn't pass them as flags (`--oidc-issuer-url`, `--oidc-client-id`, `--oidc-client-secret`, `--admin-email`, `--admin-name`).

**If a step fails partway through**, `provision` prints exactly which resources now exist and which do not — nothing is left for you to guess at. Fix the reported cause and re-run; `provision` does not yet reconcile an existing deployment (first-run scope, subject 37), so a partial `crates/*/wrangler.toml` from a failed run should be removed before retrying, or pass `--force` to let `provision` overwrite it.

**KV namespace ids have no machine-readable `wrangler` output** (`kv namespace list`/`create` document no `--json`, unlike `d1 info`) — `provision` will print the namespace it just created and prompt you to paste the id shown, rather than guessing or scraping the table.

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

## Appendix: the manual walkthrough

What `provision --apply` does, spelled out — for anyone who wants to know before running it, needs to diverge from it (a resource that already exists under a different name, an account policy `provision` doesn't account for), or is debugging a partial run.

### 1. Copy the Wrangler config templates

Neither worker's `wrangler.toml` is committed — copy the template and fill it in per deployment:

```bash
cp crates/core/wrangler.toml.example    crates/core/wrangler.toml
cp crates/gateway/wrangler.toml.example crates/gateway/wrangler.toml
```

Neither carries a secret value — that's the point (G-21: a value committed to a template is a value published in a public repository). Gateway's template ships `NOYE_ENV = "production"`; Core's does not set `NOYE_ENV` at all, since Core never reads it. You'll register the real secrets in steps 6 and 7 below. `crates/*/wrangler.toml` is git-ignored, so your filled-in copy never gets committed.

### 2. Cloudflare resources

```bash
# D1 database (Core uses this)
wrangler d1 create noye_db
# → record the printed database_id in crates/core/wrangler.toml
#   (or run `wrangler d1 info noye_db --json` afterward for the same value)

# KV namespace (Gateway uses this for sessions and JWKS cache)
cd crates/gateway && wrangler kv namespace create CACHE_KV && cd ../..
# → record the printed id in crates/gateway/wrangler.toml

# R2 bucket (Core archives logs here)
wrangler r2 bucket create noye-logs
```

### 3. D1 schema migration

The migration files live in `sql/`. The Core's `wrangler.toml` references the directory via `migrations_dir = "../../sql"`. Wrangler applies them in alphanumeric order (`0001_initial.sql`, `0003_...`, …) — `0002` is a retired, intentionally-skipped number (see `docs/src/decision-log.md` DEC-010) and must not be reused.

```bash
cd crates/core
wrangler d1 migrations apply noye_db
cd ../..
```

### 4. OIDC provider configuration

Pick an OpenID Connect-compliant provider and create an OAuth client there:

- Redirect URI: `https://<gateway-worker-domain>/auth/callback`
- Scopes: `openid`, `email`, `profile`
- Grant type: Authorization Code (PKCE-capable)

In `crates/gateway/wrangler.toml` `[vars]`, set:

- `OIDC_ISSUER_URL`
- `OIDC_CLIENT_ID`
- `OIDC_REDIRECT_URI`

The provider-specific issuer URL formats are listed in [oidc-providers.md](oidc-providers.md).

### 5. Shared secret between Gateway and Core

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

Both workers fail-close when this secret is missing — without it, every Service Binding call is rejected with FORBIDDEN. Neither template carries a fallback value, so there is nothing to remove before deploying — the check that used to require a manual "switch to production" step now applies unconditionally, in every environment (see [security-posture.md](security-posture.md#leaked-dev-fallback-detection)).

### 6. OIDC client secret

The OIDC client secret is registered only on the Gateway:

```bash
cd crates/gateway
wrangler secret put OIDC_CLIENT_SECRET
# (paste the client secret when prompted)
cd ../..
```

### 7. Initial admin user

Noye does not auto-provision users. Create the first admin with the CLI, which is idempotent — safe to re-run if you're unsure whether it already ran:

```bash
noye admin create --email admin@example.com --name Admin --remote
```

The email value must match the email claim issued by your OIDC provider for that account.

*(Older versions of this doc had you hand-write `wrangler d1 execute ... INSERT INTO users ... VALUES ('admin-001', ...)` — don't do that: a hardcoded id collides on a second run, where `admin create`'s `INSERT OR IGNORE` does not.)*

### 8. Deploy

Order matters: the Gateway's Service Binding to the Core requires the Core to exist already.

```bash
cd crates/core    && wrangler deploy && cd ../..
cd crates/gateway && wrangler deploy && cd ../..
```
