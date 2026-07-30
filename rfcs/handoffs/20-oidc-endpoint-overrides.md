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

## Done

- All four tests pass; two baseline failures captured
- `docs/src/external-design.md` §9.1 lists the three new keys
- `docs/src/requirements.md`: FR-AUTH-02, FR-AUTH-03 → `Implemented`, G-19 struck

**→ Cut v0.29.0 (M2) after subjects 08–20 are merged.** This is the first
deployable release: before tagging, run the full gate set plus a
provisioning rehearsal from an empty database, and capture both into
`.git-exclude/evidence/release-0.29.0.log` — including the complete
must-fail-first register across subjects 01–20.

## Escalate

T-102 failing under any configuration → requirements architect, immediately.
