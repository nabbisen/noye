# RFC 0003: Turnstile activation on `/auth/login`

**Status**: proposed
**Author**: nabbisen
**Last updated**: 2026-05-04
**Related ROADMAP item**: "Turnstile activation" under `## Feature`
**Estimated size**: small
**Implementation target**: post-0.27.x

---

## Summary

Wire the existing `gateway::auth::turnstile` scaffold into the
`/auth/login` page. The widget renders inline, the form blocks until
Cloudflare verifies the token, and the verification result is checked
server-side before the OIDC redirect dance begins. Activation is
gated behind an environment variable so deployments without a
Turnstile site-key continue to work unchanged.

## Background

The integration code (`gateway::auth::turnstile::verify_token`) was
landed earlier as scaffolding. It accepts a token, calls
`https://challenges.cloudflare.com/turnstile/v0/siteverify`, and
returns a verified-or-rejected result. Nothing wires it to a page yet.

The current `/auth/login` page is protected by an IP-based rate limit
(10/min, 50/hour). Turnstile adds a second layer that distinguishes
human users from automated traffic regardless of IP — useful when an
attacker has a botnet pool large enough to stay under the per-IP
ceiling.

## Design

### Configuration

Two new environment variables on the Gateway Worker:

| Variable | Type | Required when |
|---|---|---|
| `TURNSTILE_SITE_KEY` | var | Turnstile is active |
| `TURNSTILE_SECRET_KEY` | secret | Turnstile is active |

Both being set turns the feature on; either being absent disables it
(no widget, no verification). This keeps the dev-idp path
configuration-free and lets operators activate per-environment.

### Login page changes

When active:

1. The login page injects the Turnstile widget script tag and a
   `<div class="cf-turnstile" data-sitekey="..." data-callback="...">`
   placeholder inside the existing login form.
2. The "Continue with provider" button is disabled until the
   Turnstile callback fires.
3. The callback writes the verification token to a hidden form input
   `cf-turnstile-response` and re-enables the submit button.

When inactive: the page renders exactly as today (no widget, no
disabled submit).

### Submission handler change

`POST /auth/login` (the OIDC kickoff endpoint) reads the
`cf-turnstile-response` form field. When Turnstile is active:

- Missing token → `400`.
- Invalid token (siteverify rejects) → `400` with a calm error
  message.
- Valid token → proceed with the existing OIDC kickoff.

When Turnstile is inactive (env vars absent), the field is ignored.

### Pure helper

A `gateway::auth::turnstile::is_active(env)` pure helper returns
`true` iff both env vars are non-empty. Both the page renderer and the
submit handler call it; this keeps the on/off decision in one place.

### ABDD compliance

The widget itself is provided by Cloudflare and renders an iframe.
That part is outside our control. Around it, the page MUST:

- Carry an explicit `<label>` describing the human-verification step
  for screen readers.
- Disable the submit button via the `disabled` attribute (not just CSS),
  so keyboard activation respects the gate.
- Present the rejection message inline (`role="alert"`) rather than as
  a flashed redirect, so operators using assistive tech catch the
  failure mode.

## Requirements

- When both env vars are set, the login page MUST render the widget
  and the submit handler MUST enforce verification.
- When either env var is absent, the page MUST render exactly as
  before and the submit handler MUST NOT touch the
  `cf-turnstile-response` field.
- A failed verification MUST surface inline on the same page (no
  redirect, no flashed cookie) and MUST NOT advance to the OIDC
  redirect.
- The submit button MUST be disabled (HTML attribute) until the widget
  reports success, so keyboard-only users cannot bypass it via Enter
  on the form.
- A rejection message MUST be calm-toned (no all-caps, no emoji
  decoration) consistent with the `inline_result` panel style.
- The activation toggle MUST be unit-testable via the
  `is_active(env)` pure helper without spinning up a Worker.

## Test plan

### Host unit tests (target: `gateway::auth::turnstile`)

- `is_active_returns_false_when_either_env_var_missing`.
- `is_active_returns_false_when_env_vars_empty_strings`.
- `is_active_returns_true_only_when_both_env_vars_non_empty`.
- `verify_token_unit_test_with_mock_http_client_for_success_response`.
- `verify_token_unit_test_with_mock_http_client_for_failure_response`.
- `verify_token_returns_error_for_network_failure`.

### Host unit tests (target: `gateway::ui::auth::login`)

- `login_page_omits_turnstile_widget_when_inactive`.
- `login_page_emits_turnstile_widget_when_active`.
- `login_page_submit_button_carries_disabled_attribute_when_active`.
- `login_page_carries_no_disabled_when_inactive`.

### Manual / smoke

- Configure both env vars in staging, confirm the widget renders and
  that the submit blocks until verification.
- Drop `TURNSTILE_SECRET_KEY` and confirm the page reverts to the
  no-widget shape.

## Security considerations

- **Bypass via direct POST.** Without the widget, an attacker would
  POST `/auth/login` directly with a forged `cf-turnstile-response`
  field. The siteverify call rejects this; the failure path returns
  `400` and does not move into OIDC.
- **Replay.** Cloudflare's siteverify rejects already-redeemed tokens.
  Our handler relies on this rather than implementing nonce tracking
  itself.
- **Privacy.** The widget makes a request to Cloudflare which sees the
  visitor's IP and a User-Agent. This is documented in
  `docs/src/security-posture.md` (existing) and the runbook update is
  the only doc-side change needed.
- **DoS via siteverify.** Each rejected login spends a siteverify call.
  Cloudflare's rate limits apply here, but a sustained attack might
  degrade the login flow. The IP rate limit on `/auth/login`
  (existing) reduces this to a noise level.

## Out of scope

- Activating Turnstile on any endpoint other than `/auth/login`. The
  ROADMAP entry is specifically about login.
- Replacing the IP rate limit with Turnstile. Both layers stay; they
  defend against different threat shapes.
- Custom themes or sizes of the widget — Cloudflare's defaults match
  Noye's design closely enough.

## Migration / rollout notes

- No code path is exercised until both env vars are set, so a
  deployment without them sees zero behaviour change.
- Recommended rollout order: set both env vars in development first,
  exercise login end-to-end, then enable in staging, then in
  production. Each step is reversible by unsetting the env vars.
