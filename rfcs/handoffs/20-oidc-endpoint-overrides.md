# 20 — Per-endpoint OIDC overrides

**Milestone** M2 · **Closes** G-19 · **Satisfies** FR-AUTH-03, FR-AUTH-02
**Branch** `fix/20-oidc-overrides` · **Depends on** subject 19
**Governing artifact** — Gap **G-19** (§11)

## The defect

`docs/src/requirements.md` claimed these existed and were `Implemented`.
They do not exist. `crates/gateway/src/auth/oidc.rs` reads only
`OIDC_ISSUER_URL`, `OIDC_CLIENT_ID`, `OIDC_CLIENT_SECRET`,
`OIDC_REDIRECT_URI`, `OIDC_SCOPES`.

Endpoint resolution is discovery-only, so **a provider that does not
publish a discovery document is unsupported** — which makes FR-AUTH-02's
"any standards-conformant OIDC provider" overstated. That is a
deployment-blocking surprise for an operator who chose Noye partly on
that claim.

## Build

Add `OIDC_AUTH_URL`, `OIDC_TOKEN_URL`, `OIDC_JWKS_URL`. When set, each
overrides the corresponding discovered endpoint; when unset, discovery
applies as today.

**These are new configuration keys and therefore external contract** —
add them to `docs/src/external-design.md` §9.1 **before** implementing,
per that document's §14.

### The care this needs

Signature verification must run against whichever JWKS was actually used.
An override path that skips verification, or verifies against the
discovered key while accepting a token signed for the overridden one,
accepts forged tokens.

## Verify

| # | Test | Type |
|---|---|---|
| T-99 | With all three overrides set, no discovery request is made | **must fail first** |
| T-100 | With none set, behaviour is unchanged | guard |
| T-101 | With some set, the remainder still come from discovery | **must fail first** |
| T-102 | A token signed by an unknown key is rejected under **each** configuration | **guard — critical** |

**T-102 must run three times** — all overrides, none, and partial. It is
the assertion that this subject did not open a token-forgery path.

> **⚠️ T-102 belongs in the `noye-gateway` wasm test suite, not the host
> suite.** JWKS signature verification runs through Web Crypto
> (`subtle`) — the boundary **G-42** lived at, where `sha256()` could
> never succeed and no host test noticed for three releases. A
> host-target test of token verification is a test of a mock. Unlike
> `noye-core` (G-37), `noye-gateway` *can* run wasm tests, so there is no
> excuse here.
>
> T-99, T-100 and T-101 are about which URL is used and are fine as host
> tests.

## Done

- All four tests pass; two baseline failures captured
- `docs/src/external-design.md` §9.1 lists the three new keys
- `docs/src/requirements.md`: FR-AUTH-02, FR-AUTH-03 → `Implemented`, G-19 struck
- `cargo test -p noye-shared -p noye-gateway --target wasm32-unknown-unknown --lib --locked` — the wasm suites, not just `cargo check` (standing rule 8)

**→ Cut the M2 release after subjects 08–20 are merged.** Before tagging,
run the full gate set plus a provisioning rehearsal from an empty
database, and capture both into `.git-exclude/evidence/` — including the
complete must-fail-first register across subjects 01–20.

> **Version corrected 2026-08-13.** This said *"Cut v0.29.0 (M2)"*.
> **0.29.0 shipped as M1** on 2026-08-05, and M2a shipped as **0.31.0**;
> M2b and M2c are unreleased. The number is decided at release time from
> what actually changed (README, *"versions after M1 are provisional"*),
> so the milestone is named here instead of a number.

## Escalate

T-102 failing under any configuration → requirements architect, immediately.
