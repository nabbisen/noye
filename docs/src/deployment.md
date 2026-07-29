# Deployment

This document covers ongoing deployments to Cloudflare: pre-flight, deploy order, environments, rollouts, rollbacks, and the schema migration playbook.

**Lifecycle context:**

- For a one-time first deploy from a fresh checkout, see [setup.md](setup.md).
- For moving an already-deployed Noye to a different Cloudflare account, see [migration.md](migration.md).
- For local development without Cloudflare, see the [README Quick Start](../README.md#quick-start).

**Related operational docs:**

- [deployment-secrets.md](deployment-secrets.md) — secret lifecycle and rotation
- [deployment-troubleshooting.md](deployment-troubleshooting.md) — diagnosing common failure modes
- [deployment-observability.md](deployment-observability.md) — monitoring Noye itself

## Pre-flight checklist

Before every deployment to a production environment, walk through this list. None of the items are optional in production; they take a few minutes total and they catch the failure modes most likely to bite at deployment time.

- [ ] `cargo check --workspace` — compiles cleanly with zero warnings
- [ ] `cargo test --workspace --lib` — all 99 tests pass on the host
- [ ] `worker-build --release` — succeeds for both workers (Wrangler does this automatically; running it locally first surfaces toolchain or wasm-target problems faster than discovering them mid-deploy)
- [ ] `git status` — working tree is clean and you know what's in this deployment
- [ ] Schema changes, if any, have a corresponding migration file under `sql/`
- [ ] No new secret has been introduced without being registered on every environment that needs it (compare against the [deployment-secrets.md](deployment-secrets.md) inventory)
- [ ] The previous deployment has been stable for at least one full Cron cycle (60 seconds) — a quick sanity check that nothing was broken in flight
- [ ] You have access to the Cloudflare dashboard for the account you are deploying to (needed for rollback if something goes wrong)

## Deploy order

The Gateway's `wrangler.toml` declares a Service Binding to the Core. Cloudflare requires the target service to exist before the binding can be created, so on a fresh environment you must deploy the Core first:

```bash
cd crates/core    && wrangler deploy && cd ../..
cd crates/gateway && wrangler deploy && cd ../..
```

Re-deployments do not strictly require this order, but for any deployment that includes both workers, sticking to "Core first, Gateway second" is a good habit:

- If the Gateway depends on a new Core endpoint, the Core must be up before the Gateway starts calling it.
- If the Gateway has not changed, deploying only the Core is fine and is the most common case (the Core hosts most of the business logic).
- If only the Gateway has changed, you can deploy it alone and skip the Core step.

## Build

Each worker's `wrangler.toml` runs `worker-build --release` automatically as the build command. To pre-warm the build (and surface toolchain problems before the deploy step):

```bash
cd crates/core    && worker-build --release && cd ../..
cd crates/gateway && worker-build --release && cd ../..
```

`worker-build` wraps `cargo build --target wasm32-unknown-unknown --release` and generates the JavaScript shim that Wrangler uploads. Output lives in each worker's `build/` directory.

## Environments

Cloudflare Workers supports per-environment configuration via the `[env.<name>]` table in `wrangler.toml`. Noye does not ship with any environment overrides preconfigured because the binding values (D1 IDs, KV namespace IDs, route hostnames) are deployment-specific. The recommended pattern when you outgrow a single environment:

```toml
# In crates/gateway/wrangler.toml
[env.staging]
workers_dev = true
[env.staging.vars]
OIDC_REDIRECT_URI = "https://noye-gateway-staging.example.com/auth/callback"

[env.production]
workers_dev = false
route = { pattern = "noye.example.com", custom_domain = true }
[env.production.vars]
OIDC_REDIRECT_URI = "https://noye.example.com/auth/callback"
```

Deploy with the explicit environment flag:

```bash
wrangler deploy --env staging
wrangler deploy --env production
```

Every secret must be registered separately for each environment with `wrangler secret put <NAME> --env <env>`. The same applies to D1, KV, and R2 bindings — each environment should have its own resources to keep blast radius bounded.

## Rollout strategy

Cloudflare Workers deployments are atomic: a successful `wrangler deploy` switches all traffic to the new version in a single step (a few seconds of propagation across edge nodes). There is no built-in canary or percentage-based rollout for Workers. Two practical implications:

1. **There is no "deploy and watch 10% of traffic" pattern.** What you can do instead is deploy first to a staging environment (with `--env staging`), exercise the change with synthetic traffic or a manual walk-through, and then deploy to production once the staging environment looks healthy. Use staging environments aggressively for any change that touches authentication, state transitions, or notification dispatch.

2. **Lean on the deployment history for rollback** rather than hot-patching forward. The Cloudflare dashboard preserves recent deployments and you can roll back with one click. Forward-fixing a broken production deploy is almost always slower than rolling back and root-causing the issue offline.

## Rollback

### Code rollback

Open the Cloudflare dashboard → Workers & Pages → select the worker → **Deployments** tab → find the previous successful deployment → **Rollback**. This is instant and atomic, the same way deployments are.

If you do not have dashboard access, you can also redeploy a known-good git revision:

```bash
git checkout <known-good-sha>
cd crates/core    && wrangler deploy && cd ../..
cd crates/gateway && wrangler deploy && cd ../..
git checkout main
```

### Schema rollback

D1 migrations are intentionally forward-only. The migration filename convention (`0001_initial.sql`, `0003_add_field.sql`, …) means Cloudflare records which files have run and will not re-run them. `0002` is a retired, intentionally-skipped number (`docs/src/decision-log.md` DEC-010) — do not reuse it for a new migration.

Treat schema changes as additive: add columns with defaults, do not drop columns, do not rename columns in place. If you need to remove a field, deploy in two steps:

1. Stop writing to the field, deploy code, observe.
2. Schedule a separate migration to drop the column once you are confident no code path references it.

If a migration goes wrong and you need to undo it, you must write a new corrective migration (e.g. `0007_revert_0006.sql`). There is no `wrangler d1 migrations rollback`.

## Schema migration playbook

The schema directory `sql/` is shared between the workspace and the Core's wrangler config (`migrations_dir = "../../sql"` in `crates/core/wrangler.toml`). To roll out a schema change:

```bash
# 1. Add the new file with the next ordinal:
$EDITOR sql/0008_add_target_priority.sql

# 2. Apply locally first (against the local D1 simulator) to validate syntax:
cd crates/core && wrangler d1 migrations apply noye_db --local && cd ../..

# 3. Apply to the remote database. Cloudflare records which migrations have run.
cd crates/core && wrangler d1 migrations apply noye_db && cd ../..

# 4. Deploy the code that depends on the new schema (Core first):
cd crates/core    && wrangler deploy && cd ../..
cd crates/gateway && wrangler deploy && cd ../..
```

Migration files should be small and self-contained. Mixing schema and data migrations in one file makes failures harder to recover from; split them when possible.

## Health check

Both workers expose `/healthz`. The Gateway's is reachable from the public Internet:

```bash
curl https://<gateway-domain>/healthz   # → 200 "ok"
```

The Core's `/healthz` is not reachable from outside; verify it indirectly by sending an authenticated request that requires Core access (any UI page after login). A 502 or 503 from the Gateway on an authenticated page typically means the Core is down or the Service Binding is misconfigured.

## Operational notes

- **Cron schedule.** Core runs `* * * * *` (every minute). The scheduler picks up only targets whose `next_check_at` has been reached, so increasing the number of monitored targets does not increase the Cron frequency — it just shifts which targets get drained per invocation.

- **Cleanup pass.** Once an hour (at minute 0), the scheduled handler runs the data-retention pass. Rows older than the configured retention window are archived to R2 (when configured) and removed from D1.

- **Cost considerations.** Workers usage scales with request count and CPU time. The biggest knobs:
  - Cron invocations: 1 per minute = 1,440 per day per worker = ~45,000/month.
  - Per-target check requests: each due target produces one outbound `fetch` from the Core, which counts as a sub-request.
  - Service Binding hops: each authenticated UI request triggers at least 2 Service Binding calls (auth lookup + the actual data operation). UI traffic dominates request count once the system has more than a handful of users.

  The Free tier (100K requests/day) supports a small deployment with single-digit operators and a few dozen targets. For anything larger, the Workers Paid plan ($5/month for 10M requests) is the practical floor.

- **R2 archive growth.** Default retention archives `check_results` and `incidents` to R2 after 90 / 365 days. R2 storage is $0.015/GB/month and egress is free for traffic to Cloudflare services. With 100 targets at 5-minute intervals, expect roughly 1 GB of `check_results` JSONL per year before compression — not a cost concern for any reasonable scale.
