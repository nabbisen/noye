# 03 — Configuration defaults to production

**Milestone** M0 · **Closes** G-21 · **Satisfies** NFR-SEC-14, NFR-SEC-15
**Branch** `fix/03-config-templates` · **Depends on** nothing
**Governing artifact** — Gap **G-21** (§11)

## The defect

`crates/core/wrangler.toml` ships `NOYE_ENV = "development"` and
`GATEWAY_SHARED_TOKEN = "noye-local-dev-shared-token"`.
`crates/gateway/wrangler.toml:4` ships `workers_dev = true`.

`check_no_leaked_dev_fallbacks` returns `Ok(())` early when
`NOYE_ENV == "development"` — which the shipped file sets. **The control
is disabled by the configuration it exists to protect.**

Deploy the repository unmodified and you get a permissive environment
whose inter-Worker authentication is a value published in a public
repository. The guard cannot fire, by construction.

## Build

1. Rename both `wrangler.toml` → `wrangler.toml.example`. Add
   `crates/*/wrangler.toml` to `.gitignore`.
2. In both templates: `NOYE_ENV = "production"`; Gateway
   `workers_dev = false` with a comment on configuring a route;
   **remove `GATEWAY_SHARED_TOKEN` and `OIDC_CLIENT_SECRET` values
   entirely**, leaving commented instructions pointing at
   `wrangler secret put` for deployment and `.dev.vars` for local work.
3. In `crates/gateway/src/env_check.rs`, delete the early return on
   `is_development()`. The denylist applies **unconditionally**.
4. Update the `KNOWN_DEV_FALLBACKS` doc comment — it says the values
   "ship in `crates/gateway/wrangler.toml`'s `[vars]` block" and are
   "expected" in development. Neither will be true. Add: **entries are
   never removed**, because a value published once stays published.

### Notes

`.dev.vars` is already in `.gitignore:35`, so the local path needs
documentation, not machinery. Local development generates its own token.

A developer whose `.dev.vars` holds a denylisted value will now be
refused. That is correct, and it is the one case where someone may be
briefly confused — the error wording matters more than usual.

Preserve the existing property: the error names the variable, never logs
the value.

### Do not

Do not rename any configuration **key**. Names are an external contract
(external design §14); renaming one breaks deployed environments. Only
how they are supplied changes.

## Verify

| # | Test | Type |
|---|---|---|
| T-10 | No `wrangler.toml` tracked; both `.example` files tracked; neither contains a denylisted value | **must fail first** |
| T-11 | A denylisted credential is refused when `NOYE_ENV = development` | **must fail first** |
| T-12 | …when `production` | guard |
| T-13 | …when `NOYE_ENV` is unset — also guards NFR-SEC-07 | guard |
| T-14 | A locally generated, non-denylisted token authenticates Gateway → Core | guard |
| T-15 | The refusal message names the variable and contains no value | guard |

## Done

- All six tests pass; two baseline failures captured
- `docs/src/setup.md` gains the copy-the-template step
- `docs/src/development.md` gains the `.dev.vars` step including token generation
- A clean checkout following those documents yields a working local stack
- `docs/src/security-posture.md` records the unconditional denylist
- `docs/src/requirements.md`: NFR-SEC-14, NFR-SEC-15 → `Implemented`,
  NFR-SEC-09 → `Implemented`, G-21 struck

**→ Cut v0.28.0 (M0) after subjects 01–03 are merged.**

## Escalate

A deployment path exists that bypasses `env_check` → requirements architect.
