# Email delivery

Email is one of the three notification transports Noye supports (alongside
generic webhooks and Slack incoming webhooks). It is **fully active** as of
0.17.0. powered by the [`wasm-smtp`](https://crates.io/crates/wasm-smtp)
+ [`wasm-smtp-cloudflare`](https://crates.io/crates/wasm-smtp-cloudflare)
crates (pinned to `=0.9.3` since 0.21.0 — see "Authentication mechanisms"
below for the security improvement that release brought). Message
composition itself moved to
[`mail-builder`](https://crates.io/crates/mail-builder) in 0.22.0. replacing
roughly 80 lines of hand-rolled RFC 5322 / RFC 2047 logic.

## Configuration

In `crates/core/wrangler.toml`:

```toml
[vars]
EMAIL_SMTP_HOST     = "smtp.example.com"
EMAIL_SMTP_PORT     = "587"
EMAIL_SMTP_USERNAME = "noye"
EMAIL_SMTP_TLS_MODE = ""           # optional; "implicit" or "starttls"
EMAIL_FROM_ADDRESS  = "noye@example.com"
EMAIL_FROM_NAME     = "Noye Monitoring"
```

The matching secret on the Core:

```bash
cd crates/core
wrangler secret put EMAIL_SMTP_PASSWORD
# (paste the SMTP password / API token at the prompt)
```

For multi-environment deployments (`--env staging`, `--env production`),
register the secret per environment.

### TLS mode auto-detection

`EMAIL_SMTP_TLS_MODE` is optional. When empty the mode is derived from the
port:

| Port | Mode | Wire protocol |
|---|---|---|
| 465 | Implicit TLS (SMTPS) | TLS handshake before the SMTP banner; sometimes called "SMTPS" |
| anything else | STARTTLS | Plaintext connect, EHLO, then upgrade to TLS via `STARTTLS` |

Set `EMAIL_SMTP_TLS_MODE` explicitly only if your relay uses a non-standard
port. Accepted values: `"implicit"` (also `"smtps"`, `"tls"`) or `"starttls"`.
Anything else falls back to port-based auto-detection.

## State table

| State | Cron dispatch | Test send (`POST /channels/:id/test`) |
|---|---|---|
| `EMAIL_SMTP_HOST` is empty | Logs `[email-disabled]` and returns OK; no message sent | Returns an explicit "not configured" error |
| `EMAIL_SMTP_HOST` is set but other values are missing or malformed | Returns a misconfiguration error from the dispatch loop (logged via `console_error!`) | Returns the same error to the operator |
| All values configured | Sends via `wasm-smtp`; logs SMTP errors on failure | Returns `Ok` on accepted delivery, surfaces the SMTP error message on failure |

The deliberate property is that *no email channel ever silently appears to
work*. Every state where a real send is impossible produces a diagnostic
either in the operator's UI (test send) or in `console_error!` logs (Cron).

## Provider examples

Any SMTP-AUTH relay that supports either Implicit TLS (465) or STARTTLS (587)
works. Some common ones:

| Provider | Host | Port | Mode | Username | Notes |
|---|---|---|---|---|---|
| AWS SES (us-east-1) | `email-smtp.us-east-1.amazonaws.com` | 587 | STARTTLS | IAM SMTP credentials | Verify the from-address or domain in SES first |
| SendGrid | `smtp.sendgrid.net` | 587 | STARTTLS | `apikey` (literally) | Password is the API key |
| Mailgun (US) | `smtp.mailgun.org` | 587 | STARTTLS | `postmaster@<domain>` | Sandbox domains have recipient allow-lists |
| Resend | `smtp.resend.com` | 465 | Implicit | `resend` | Auto-detected via port 465 |
| Postmark | `smtp.postmarkapp.com` | 587 | STARTTLS | server token | Same value for username and password |

Self-hosted relays (Postfix, OpenSMTPD) work too if they expose SMTP-AUTH.

## Authentication mechanisms

`wasm-smtp` 0.9.x picks the strongest mechanism the server advertises in
`EHLO`'s `AUTH` line, in this priority order:

1. **`SCRAM-SHA-256`** (RFC 7677) — challenge-response SASL, the password
   never crosses the wire in plaintext. Default-on since wasm-smtp 0.9.0.
2. **`PLAIN`** — base64-encoded `user\0user\0password` over the (already
   TLS-encrypted) connection.
3. **`LOGIN`** — legacy two-step base64 challenge.

For Noye's typical relay landscape:

| Relay class | Advertised mechanisms (typical) | Effective Noye behavior |
|---|---|---|
| Cloud transactional senders (SES, SendGrid, Mailgun, Postmark, Resend) | `PLAIN`, `LOGIN` | Falls through to `PLAIN` over TLS — same as before 0.21.0 |
| Self-hosted modern Postfix / Stalwart / Dovecot | `SCRAM-SHA-256`, `PLAIN`, `LOGIN` | Auto-upgrades to SCRAM — passwords no longer transit even encrypted |
| Legacy / restricted setups | `LOGIN` only | Uses `LOGIN` |

This is a transparent improvement: `EMAIL_SMTP_PASSWORD` rotation, configuration,
and the credentials registered with each provider remain unchanged. The mechanism
selection happens server-side via the `EHLO` advertisement and is invisible to
operators.

If a self-hosted relay's SCRAM implementation has bugs (rare, but it's a newer
mechanism than `PLAIN`), the symptom is an `AuthError::Other(...)` log line
naming the SCRAM step that failed (`server-nonce mismatch`,
`iteration count out of bounds`, `server signature verification failure`).
The workaround would be a `wasm-smtp` configuration knob to force `PLAIN`,
which Noye does not currently expose; for now, a confirmed SCRAM bug on a
specific relay would warrant filing an upstream report.

## Message format

Since 0.22.0 message composition is delegated to
[`mail-builder`](https://crates.io/crates/mail-builder) (Stalwart Labs).
Noye supplies a small set of fields and the library handles the RFC 5322
mechanics (line folding, encoded-word selection, CRLF normalization, MIME
headers).

What Noye specifies on each notification:

- **From** — display name + address from `EMAIL_FROM_NAME` / `EMAIL_FROM_ADDRESS`. The display name is dropped when empty so the renderer doesn't emit an empty quoted pair.
- **To** — the channel's configured recipient address (validated by `is_valid_email` before opening the connection).
- **Subject** — passed through as a UTF-8 string. mail-builder picks Q- or B-encoding (RFC 2047) automatically when non-ASCII characters are present; pure-ASCII subjects pass through unchanged.
- **Date** — current Unix timestamp passed via `mail_builder::headers::date::Date::new(i64)`. Set explicitly (not via `Date::now()`) because that path calls `SystemTime::now()` which is not always reliable on `wasm32-unknown-unknown`.
- **Message-ID** — `<uuid-v4@from-address-domain>`. Overrides mail-builder's auto-generated path so the domain matches the From address (some relays reject otherwise). Also avoids pulling in mail-builder's `gethostname` feature (which is incompatible with WASM).
- **Body** — plain text; mail-builder handles CRLF normalization and the MIME headers (`MIME-Version: 1.0`, `Content-Type: text/plain; charset=utf-8`, `Content-Transfer-Encoding: ...`).
- **X-Mailer** — `noye-monitoring`, attached as a raw header so log auditors can identify Noye-originated mail at a glance.

The composition lives in `crates/core/src/notify/email.rs::build_message`,
extracted as a free function so it is unit-testable without an SMTP
runtime — the tests serialize the builder via `write_to_string` and
assert on the rendered headers (8 unit tests covering: From with name,
empty-name omission, To/Subject/Message-ID/Date presence, X-Mailer
header, blank-line CRLF separator, body passthrough, encoded-word
framing for non-ASCII subjects).

## Sender-domain setup (DKIM / SPF)

Without DKIM and SPF aligned to your sending domain, most receiving servers
will route alerts to spam. This is a deployment-side concern, not a Noye
concern, but it is on the operator's checklist:

1. **SPF.** Add a TXT record at the apex of your sending domain that
   authorizes the SMTP relay to send on your behalf. The exact wording is
   provider-specific; consult the provider's docs.
2. **DKIM.** Generate a signing key pair at the provider, publish the public
   half as a TXT record, configure the relay to sign outgoing mail with the
   private half. Most providers do this for you with a one-click setup.
3. **DMARC** (optional but recommended). A DMARC record tells receivers what
   to do with mail that fails SPF or DKIM. Start with `p=none` for
   observability, move to `p=quarantine` once you trust your alignment.

## Operational notes

- **Throughput.** SMTP-AUTH connections are stateful; each `send_email` call
  opens a connection and closes it. For deployments with many noisy targets,
  this is wasteful but acceptable up to ~10 alerts/minute. Beyond that, a
  pooled SMTP client (or a switch to a provider's HTTP API) would be the
  upgrade path.
- **Cron CPU budget.** Workers caps Cron-handler CPU at 30 ms (paid plan).
  An SMTP round-trip including TLS handshake comfortably fits, but ten of
  them in sequence might not. The current scheduler processes targets
  sequentially; if you reach ten parallel email-bound notifications per Cron
  tick, consider Cloudflare Queues to fan the work out.
- **Bounces.** Noye does not consume bounce notifications. The provider's
  dashboard is the source of truth for whether a particular recipient is
  reachable. A "channel that has been bouncing for a week" looks identical
  to a "healthy channel" from inside Noye.
- **TLS errors.** A failed TLS handshake (expired cert, unsupported version,
  hostname mismatch) surfaces as `smtp transport error: ...` from the
  underlying transport. Verify with `openssl s_client -connect host:port` or
  `openssl s_client -starttls smtp -connect host:587`.

## Architecture: why three crates

Email delivery is split between three upstream crates:

- **`wasm-smtp`** — pure-protocol crate: SMTP state machine, command and
  response parsing, dot-stuffing, error classification. No I/O. Reusable on
  any runtime that can supply a `Transport` impl.
- **`wasm-smtp-cloudflare`** — adapter crate: bridges Cloudflare Workers'
  `worker::Socket` to `wasm-smtp::Transport`. Implements TLS handshake (both
  Implicit and STARTTLS) using Cloudflare's `SecureTransport` knobs.
- **`mail-builder`** — RFC 5322 / MIME composition crate. Zero required
  dependencies (we disable its default `gethostname` feature for WASM
  compatibility). Used since 0.22.0 in place of Noye-side hand-rolled
  message composition.

Noye depends on all three. The `connect_smtps` and `connect_smtp_starttls`
functions in the adapter return a fully-formed `SmtpClient<...>` that we
drive directly from `notify::email::send_email`. Composition flows
through `mail-builder` to a `MessageBuilder`, which `SmtpClient::send_message`
serializes via `write_to_string` and forwards to `send_mail`.

### Build-time configuration

The `scram-sha-256` cargo feature (default-on as of `wasm-smtp` 0.9.0)
pulls in `getrandom` 0.4 for client-nonce generation. On
`wasm32-unknown-unknown` (the Workers target), `getrandom` 0.4 requires
a rustc `--cfg` flag that cannot be expressed as a cargo feature.

Noye sets it in `.cargo/config.toml` at the workspace root:

```toml
[target.wasm32-unknown-unknown]
rustflags = ['--cfg=getrandom_backend="wasm_js"']
```

This was added in 0.21.0 alongside the `wasm-smtp` 0.6 → 0.9.3 bump.
Anyone building Noye from source picks it up automatically — it lives in
the repo's `.cargo/config.toml`, no environment variable export is needed.

If the SCRAM feature were ever disabled (`default-features = false` on
`wasm-smtp-cloudflare`), the `--cfg` would become harmless dead config,
because `getrandom` would no longer be in the dependency graph. Disabling
SCRAM would be a security regression on relays that advertise it, so we
do not currently offer that knob.

### `mail-builder` feature configuration

`mail-builder` ships with its `gethostname` feature default-on, which
pulls in a syscall-using crate that does not compile on
`wasm32-unknown-unknown`. The feature is only consumed by mail-builder's
auto-generated Message-ID path — and Noye overrides Message-ID anyway
(building it from the From-address domain so relays see a coherent
identity). Noye therefore declares the dependency with
`default-features = false`:

```toml
mail-builder = { version = "0.4", default-features = false }
```

The `mail-builder` cargo feature on `wasm-smtp-cloudflare` is also
enabled, which transitively enables it on `wasm-smtp` and unlocks
`SmtpClient::send_message(MessageBuilder)`. Without this feature
`send_message` is not compiled; we would fall back to manual
`MessageBuilder::write_to_string()` plus `client.send_mail()`. The
`send_message` shortcut keeps Noye's call site to one statement.

## See also

- [requirements.md](requirements.md) — Roadmap rationale for the
  SMTP-vs-API-provider decision.
- [api.md](api.md) — The `POST /api/channels/:id/test` endpoint that exposes
  the diagnostic flow described above.
- [deployment-secrets.md](deployment-secrets.md) — Secret rotation procedure
  for `EMAIL_SMTP_PASSWORD`.
