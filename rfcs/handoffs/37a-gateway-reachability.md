# 37a — `provision` produces a Gateway you can reach and log into

**Milestone** M2.1 · **Closes** G-48 · **Satisfies** FR-AUTH-01
**Branch** `fix/37a-gateway-reachability` · **Depends on** subject 37 (merged)
**Governing artifact** — Gap **G-48** (§11)

## The defect

`noye provision --apply` completes successfully and produces a
deployment **nobody can reach or log into.** Two parts, one fix.

**(a) Nothing makes the Gateway reachable.**
`crates/gateway/wrangler.toml.example:16` ships `workers_dev = false`
with no `[[routes]]` block, and its own comment says so outright:

> *"`workers_dev = false` here means Cloudflare's `*.workers.dev`
> subdomain is disabled — you must configure a custom route (or a zone)
> for the Gateway to be reachable at all."*

`render_gateway_config` (`crates/cli/src/provision/steps.rs:257`)
rewrites the worker name, the KV id, the service binding, the issuer URL
and the client id. It touches neither `workers_dev` nor any route.

**(b) `OIDC_REDIRECT_URI` is never rendered.** It stays at the
template's `http://localhost:8787/auth/callback`
(`wrangler.toml.example:62`), so a deployed Gateway sends every login
attempt to the operator's own machine.

**Core's `workers_dev = false` is correct and must not change.** Core is
deliberately unroutable — §2.3 of the external design calls that *"the
single most important external property."* This subject is about the
Gateway only.

### How it was missed, which is the part worth keeping

Subject 37's review (ruling `075`) verified the `Step` trait, secret
handling, id parsing and name derivation, and accepted the dev team's
own §9 — *"`--apply` against a real account has never been run"* — as a
known limitation **without asking what that run would produce.** The
defect was found on the first attempt to actually use the command.

Same class as **G-36**, **G-38** and **G-42**: reading the code did not
find it. Executing it did.

## Build

### One flag, one source of truth

```
--gateway-hostname <host>
```

It determines **both** facts, so they cannot drift apart:

| `<host>` | Reachability | `OIDC_REDIRECT_URI` |
|---|---|---|
| ends in `.workers.dev` | `workers_dev = true`, no route block | `https://<host>/auth/callback` |
| anything else | `workers_dev = false` **plus** a `[[routes]]` entry with `pattern = "<host>"` and `custom_domain = true` | `https://<host>/auth/callback` |

**Do not take the redirect URI as a separate flag.** Two inputs for one
fact is the drift subject 37 already avoided for the Service Binding,
where Gateway's `service = "..."` is always derived from Core's own
name rather than carried independently. Same reasoning here.

**Validate the workers.dev case.** When `<host>` ends in
`.workers.dev`, its first label must equal the derived gateway worker
name. `noye-verify-gateway.scqr-inc.workers.dev` with
`--env-name verify` is consistent; `noye-gateway.scqr-inc.workers.dev`
with `--env-name verify` is a copy-paste error that would deploy to one
worker and point OIDC at another. **Refuse it, naming both.**

### Required, not optional

`--apply` without `--gateway-hostname` must **fail before creating
anything**, with a message saying the Gateway would be unreachable and
naming the two forms. Defaulting silently to either shape is how (a)
happened.

`--plan` must print the resolved hostname, the reachability mechanism it
implies, and the redirect URI — so the next person can see all three
before anything exists, which is the only reason this defect was caught
at all.

## Verify

| # | Test | Type |
|---|---|---|
| T-236 | A `.workers.dev` host sets `workers_dev = true` and writes no route block | **must fail first** |
| T-237 | A custom host sets `workers_dev = false` and writes a `[[routes]]` entry with `custom_domain = true` | **must fail first** |
| T-238 | `OIDC_REDIRECT_URI` is rendered from the hostname in both shapes, and `localhost:8787` survives in neither | **must fail first** |
| T-239 | A `.workers.dev` host whose first label ≠ the derived gateway worker name is refused, naming both | **guard — critical** |
| T-240 | `--apply` without `--gateway-hostname` fails before any step runs | **guard — critical** |
| T-241 | `--plan` prints hostname, mechanism and redirect URI | guard |
| T-242 | Core's rendered config still has `workers_dev = false` and no route | **guard — critical** |

**T-242 is the one that protects the architecture.** Core being
unroutable is the external design's single most important external
property; a fix aimed at the Gateway must not reach it. Assert against
the *real* `crates/core/wrangler.toml.example`, the way subject 37's own
template tests do.

**T-239 and T-240 are the pair that keeps (a) from recurring.** One
catches a hostname that doesn't match the deployment; the other refuses
the silent default that caused the gap.

## Done

- All seven tests pass; three baseline failures captured
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` (standing rule 8)
- `docs/src/setup.md` — the `provision` invocation gains the flag, and
  the "what `provision` cannot do for you" section says you must decide
  the Gateway's hostname before running it
- `crates/gateway/wrangler.toml.example` — comment updated to say which
  values `provision` now fills, `OIDC_REDIRECT_URI` included
- `docs/src/requirements.md`: G-48 struck

## Escalate

**If a `[[routes]]` + `custom_domain = true` block cannot be validated
without an account**, build it, assert its rendered shape in a test, and
say so — the same disposition subject 37 took for the KV id. Do not
guess at route semantics and do not leave the custom-host path
unimplemented: the owner's own deployment uses a custom domain
(`noye.scqr.net`), so that path is the production one, not a courtesy.
