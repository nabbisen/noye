# Noye Roadmap

Items intentionally deferred. Each entry includes the reasoning so a
future operator picking up the work can decide whether it is still
needed and what shape it should take.

> **Detailed specifications for the priority items live in
> [`rfcs/`](rfcs/).** This roadmap stays high-level; each RFC takes one
> entry to implementer-ready depth. See [`rfcs/README.md`](rfcs/README.md)
> for the index and the workflow contract.

## UI / theme

### Manual theme toggle (light / dark / system)

**RFC**: [0001](rfcs/0001-manual-theme-toggle.md).

**Status**: deferred (since 0.23.0).

**Why deferred**: 0.23.0 introduced a token-based design system with
both light and dark presets. The active theme is currently selected by
`prefers-color-scheme` only — the OS-level setting wins. A manual
toggle would let users override the OS preference within a Noye session
(e.g. an admin who runs a dark-themed OS but wants Noye in light mode
during a daytime briefing).

**Suggested implementation when picked up**:

- Persist the preference in a cookie (`noye_theme=light|dark|system`,
  `Path=/`, `Max-Age=31536000`, `Secure` in production, `HttpOnly`
  unset because the client needs to read it).
- Read the cookie in the `wrap()` helper and write a `data-theme`
  attribute on `<html>`. Add a corresponding `[data-theme="light"]`
  selector in `style.rs` that mirrors the `prefers-color-scheme: light`
  block.
- Render a small toggle in the user-info chip (top-right) — three-state
  button cycling system → light → dark → system.
- The cookie path keeps the toggle JS-free if needed (a `<form
  method="POST" action="/me/theme">` submission could update it
  server-side); see if there's appetite for a no-JS path.

**Why not now**: would have stretched Phase A beyond its scope. Phase
A was focused on the token system itself — once the tokens are stable,
adding a third theme branch and a UI control is straightforward
incremental work.

### High-contrast mode preset

**RFC**: [0005](rfcs/0005-high-contrast-theme.md).

**Status**: deferred.

**Why deferred**: WCAG AAA (7:1 body, 4.5:1 large) is achievable with a
small token override but adds maintenance burden. None of the current
operators have requested it and the AA baseline already covers the
disability-discrimination compliance bar in most jurisdictions.

**Suggested implementation when picked up**: add a `[data-theme=
"high-contrast"]` token preset that pushes text colors closer to pure
black/white and bumps border-strong contrast. Pin the new pairs in
`contrast.rs::tests::critical_pairs_meet_aa` against the AAA threshold.

## Operations infrastructure

### Cargo.lock commit + GitHub Actions CI + cargo-audit

**Status**: ✅ Done in 0.27.0.

`Cargo.lock` is committed; `.github/workflows/ci.yml` runs format / clippy / check / host-test / WASM-build / cargo-audit on every push and PR, plus a weekly audit cron (Saturdays 02:00 UTC). See `docs/src/development.md#continuous-integration`.

### Cloudflare Logs export (audit-log mirror)

**RFC**: [0002](rfcs/0002-audit-log-mirror.md).

**Status**: deferred (operator-side configuration).

**Why deferred**: the in-D1 hash chain detects tampering, but a
wholesale `DROP TABLE audit_logs` leaves nothing to verify against. A
log-shipping mirror to an off-D1 destination is the recovery path. This
is configured at the Cloudflare level rather than in Noye code.

**Suggested implementation when picked up**: document a
`docs/operations/audit-log-mirror.md` runbook covering Logpush
configuration, retention guidance, and how to use the mirrored stream
to repair a corrupted `audit_logs` table.

## Feature

### Workers Queue fan-out for Cron monitor

**Status**: deferred (scale).

**Why deferred**: Noye's monitor engine processes targets serially
within one Cron tick; for the current scale (~ 数百 targets) this
finishes well within the one-minute window. Past ~1000 targets the
fan-out becomes necessary to keep latency bounded.

**Notes**: the audit-log hash chain is currently single-writer; if
fan-out is added, audit-log writes need a Durable Object (or an
external serialization point) to avoid chain forks.

### Turnstile activation

**RFC**: [0003](rfcs/0003-turnstile-activation.md).

**Status**: scaffolded but not wired up.

**Why deferred**: the Cloudflare Turnstile integration code exists
under `gateway::auth::turnstile` but the UI and rate-limit don't
require it today. Activation is gated on observing actual abuse against
`/auth/login` past what the IP rate limit (10/min, 50/hour) can absorb.

### Slack-specific notification payload formatting

**RFC**: [0006](rfcs/0006-slack-payload.md).

**Status**: deferred.

**Why deferred**: the current Slack channel sends the same generic JSON
as the Webhook channel. Slack accepts it but the rendering is plain.
Operators who want richer Slack messages (color attachments, action
buttons) can configure their incoming-webhook target to parse Noye's
payload, but a first-class adapter would be cleaner.

### Failed-login audit recording

**RFC**: [0004](rfcs/0004-failed-login-audit.md).

**Status**: deferred.

**Why deferred**: the OIDC callback only records *successful* login
events to `audit_logs`. Failed attempts log to `console_error!` but
don't appear in `/me/security` recent-logins or in the chain. Adding
them would be straightforward but requires deciding what to attribute
the row to (the attempted email may not match a real user; the failure
may be earlier than that).

### HTML / multipart email bodies

**Status**: deferred.

**Why deferred**: notification emails are plain text today, which is
adequate for short DOWN/UP alerts. `mail-builder` makes it
straightforward to add HTML alternatives, but no operator has asked
for it.
