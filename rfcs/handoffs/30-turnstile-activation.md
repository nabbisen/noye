# 30 — Turnstile activation

**Milestone** M5 · **Satisfies** FR-OPS-07
**Implements** [RFC 0003](../proposed/003-turnstile-activation.md)
**Branch** `rfc-0003-turnstile` · **Depends on** M4 shipped
**Governing artifact** — **RFC 0003**

## Scope

The integration code exists in `gateway::auth::turnstile` — it accepts a
token, calls the siteverify endpoint, and returns a verified-or-rejected
result. Nothing wires it to a page.

Wire it to `/auth/login`. Activation is gated on the site-key variable,
so deployments without one continue unchanged.

## Build

Per RFC 0003. The requirement is explicit on one point:

**The challenge MUST remain confined to public forms.** A challenge on an
authenticated route would be both useless and hostile — the operator has
already proven who they are.

The widget must not break the no-JavaScript baseline for anything other
than itself: login must still be reachable, and the failure mode when the
challenge cannot load must be legible rather than a dead form.

## Verify

| # | Test | Type |
|---|---|---|
| T-144 | The widget appears on `/auth/login` only, on no authenticated route | **must fail first** |
| T-145 | With no site key configured, login works exactly as before | guard |
| T-146 | A failed challenge blocks the OIDC redirect | **must fail first** |

## Done

- All three tests pass
- RFC 0003 → `rfcs/done/`, `Status: Implemented (1.0.0)`, inbound links fixed
- `docs/src/turnstile.md` updated
- `docs/src/requirements.md`: FR-OPS-07 → `Implemented`
