# Development: contributor's reference

This document is for people writing code against the Noye codebase: how the project is organized, where to put new code, how versioning and packaging work, and how the test suite is structured.

**For first-time local-run instructions** (running `wrangler dev` against an empty Miniflare environment), see the [README Quick Start](../README.md#quick-start).

**For deploying to Cloudflare**, see [setup.md](setup.md) (first time) or [deployment.md](deployment.md) (ongoing).

## Iteration loop

For a fast Rust-only iteration loop, run `cargo check --workspace` from the repo root before invoking `wrangler dev`. Most type errors and missing-import problems show up in seconds without spinning Miniflare up.

### One-time local config

Neither worker's `wrangler.toml` is committed (Subject 03 / G-21 —
`crates/*/wrangler.toml` is git-ignored). Copy the templates once:

```bash
cp crates/core/wrangler.toml.example    crates/core/wrangler.toml
cp crates/gateway/wrangler.toml.example crates/gateway/wrangler.toml
```

Gateway's template ships `NOYE_ENV = "production"`; Core's template
does not set `NOYE_ENV` at all — Core never reads it (no cookies or
sessions of its own). Neither template carries a secret value.
`check_no_leaked_dev_fallbacks` runs unconditionally in both Workers —
in every environment, not only production — so local `wrangler dev`
needs its own non-denylisted values too. Create `.dev.vars` next to
each `wrangler.toml` (git-ignored; `wrangler dev` merges it into
`[vars]` automatically):

```bash
# crates/gateway/.dev.vars
cat > crates/gateway/.dev.vars <<'EOF'
NOYE_ENV=development
OIDC_CLIENT_SECRET=anything-not-on-the-denylist
GATEWAY_SHARED_TOKEN=REPLACE_WITH_GENERATED_TOKEN
EOF

# crates/core/.dev.vars — no NOYE_ENV; Core doesn't read it
cat > crates/core/.dev.vars <<'EOF'
GATEWAY_SHARED_TOKEN=REPLACE_WITH_GENERATED_TOKEN
EOF

# Generate one token and paste it into both files above —
# GATEWAY_SHARED_TOKEN must match on both workers:
openssl rand -hex 32
```

`OIDC_CLIENT_SECRET`'s value is unchecked by `noye-dev-idp` (see
[dev-idp.md](dev-idp.md)) — any string works except the retired
default, which is now denylisted everywhere. Gateway's
`NOYE_ENV=development` only affects cookie strictness (plain-HTTP
`localhost` needs it); it no longer exempts anything from the
dev-fallback check, on either Worker.

### Running both workers

```bash
# Terminal 1: Core
cd crates/core && wrangler dev --port 8788

# Terminal 2: Gateway
cd crates/gateway && wrangler dev --port 8787
```

Wrangler v4 supports cross-worker Service Bindings during local development. The Gateway will route its `core_client` calls into your locally-running Core automatically.

## Code organization

The workspace uses the modern flat module convention (no `mod.rs`). When adding a submodule, create both `<parent>.rs` and `<parent>/<child>.rs`.

The Cargo workspace defines all dependency versions centrally in `[workspace.dependencies]`. Member crates reference them with `workspace = true`. Bumping a version in one place affects all three crates.

## Versioning

Version metadata is centralized in `[workspace.package]` at the workspace root:

```toml
[workspace.package]
version = "0.1.0"
edition = "2024"
authors = ["nabbisen"]
license = "Apache-2.0"
```

Member crates inherit these via `version.workspace = true`, `edition.workspace = true`, etc. Bumping the version is a single edit at the root.

## Packaging

`package.sh` produces a release archive named after the workspace version:

```bash
./package.sh                # writes ./dist/noye-project-v<version>.tar.gz
./package.sh /tmp/releases  # writes the archive into /tmp/releases/
```

The script reads the version through `cargo metadata`, so it always tracks `[workspace.package].version`. The archive is `git archive` over the git tag matching that version — exactly the tracked content of the tagged commit, `Cargo.lock` included (DEC-019). It refuses to run against a dirty working tree, a version with no matching tag, or a `HEAD` that isn't at the tagged commit.

For producing a *distributed* release artifact, don't run this locally — push the version tag and let `.github/workflows/release.yml` build and attach it. See [deployment.md § Release archive](deployment.md#release-archive). This local invocation is for ad-hoc inspection of what a tag's archive would contain.

## Code style

- **All comments and documentation in English.** UI string literals are also English. The codebase follows the Apache-2.0 conventions.
- **Idiomatic Rust 2024.** `let-else`, async functions, and pattern-matching are used freely. Avoid `unwrap()` outside of unrecoverable startup paths; use `?` and meaningful `Error::RustError(...)` messages.
- **Module boundaries follow domain.** Don't split a file just because it grew long; split when there is a clean conceptual boundary (e.g. one module per protocol checker, one per UI page).

## Adding a new monitor protocol

1. Add a new file under `crates/core/src/monitor/`, e.g. `monitor/mqtt.rs`.
2. Implement an `async fn check(env: &Env, target: &Target) -> CheckOutcome`.
3. Add a `pub mod mqtt;` declaration to `crates/core/src/monitor.rs`.
4. Wire the new protocol into the dispatch arm in `monitor::engine::execute_check`.
5. Add the protocol identifier to the `CHECK (type IN (...))` constraint in `sql/0001_initial.sql` (for new schemas) or as a follow-up migration.

## Adding a new UI page

1. Add a renderer module under `crates/gateway/src/ui/`, e.g. `ui/reports.rs`.
2. Add `pub mod reports;` to `crates/gateway/src/ui.rs`.
3. Add a route handler to `crates/gateway/src/lib.rs`. Use the `authenticate()` wrapper for any non-`/healthz` and non-`/auth/*` route.
4. If the page needs new data, add a corresponding `core_client` function and a Core API endpoint.

## Adding a new Core API endpoint

1. Add a handler under `crates/core/src/api/`, e.g. `api/reports.rs`.
2. Add `pub mod reports;` to `crates/core/src/api.rs`.
3. Add the route to `crates/core/src/lib.rs`.
4. Add a matching `core_client` wrapper in `crates/gateway/src/core_client.rs` that injects `X-Caller-*` headers and the gateway token.

## Testing

Noye uses standard Rust unit tests, placed at the bottom of each module under a `#[cfg(test)] mod tests` block. The convention is one test module per source file, never separated into an external `tests/` directory.

### Running the test suite

There are now **two test surfaces**: the host-target unit tests (the bulk of the suite) and the WASM-target tests that exercise Web Crypto / JS-binding code paths only reachable inside a WASM runtime.

#### Host-target tests (default)

```bash
cargo test --workspace --lib --bins
```

This covers all host-side tests across the workspace: library tests in `gateway`, `core`, `shared`, plus binary-crate tests in `cli` and `dev-idp`. The flags `--lib --bins` skip doctests, which require the unstable `--check-cfg` rustc flag and fail to run on stable rustc 1.91. We have no documented public API meant to be doc-tested, so this is not a real loss.

If you only want the gateway/core/shared library suites (faster, since this skips compiling the bin crates' RSA dependencies), use `cargo test --workspace --lib`.

#### WASM-target tests

```bash
# One-time setup
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2

# Run the tests
cargo test -p noye-gateway --target wasm32-unknown-unknown
```

The runner is configured in `.cargo/config.toml` to use `wasm-bindgen-test-runner`, which spins up Node 20+ to execute the compiled `.wasm` test binary. Node ships with the Web Crypto API enabled by default at this version, so no additional flags are needed.

These tests are gated by `#[cfg(all(test, target_arch = "wasm32"))]` and live next to the code they cover, in `mod wasm_tests` blocks at the bottom of each `auth::crypto::*` source file. They are invisible to the host runner.

#### Node is not Workers — and `wrangler dev --local` is

**The two WASM surfaces above answer different questions, and it matters
which one you used.**

`cargo test --target wasm32-unknown-unknown` runs under **Node**. That is
the right instrument for pure logic and for anything whose answer is a
language-level fact, and it is fast.

`wrangler dev --local` runs **`workerd`** — the same open-source runtime
Cloudflare deploys to production, shipped as a wrangler dependency. So
**any question of the form "does this behave the same on real Workers?"
is answerable locally**, without a deployment and without touching any
real Cloudflare account (handoff README standing rule 7).

That distinction has been load-bearing twice:

- **G-42** was observed failing under Node, and the inference to Workers
  was spec-reasoning until it was re-observed under `workerd`. The
  difference between "almost certainly behaves the same" and "observed
  behaving the same" is the difference this project keeps paying for.
- **Every D1 finding** in [The D1 type boundary](./d1-type-boundary.md)
  was produced under `wrangler dev --local` rather than under Node or
  bare `sqlite3`, which is why those rows say *"confirmed against the
  local D1 runtime"* and mean it. Bare `sqlite3` gets several of them
  wrong — it defaults `foreign_keys` to `0` and does not stop at the
  first error in a script.

**When you record a result, record which runtime produced it.** Node and
`workerd` are different answers to the same question, and a result whose
runtime is unstated cannot be checked later.

#### A behaviour gate, not just a boundary check

`wrangler dev --local` doesn't only answer boundary questions one type at
a time — subject 07f (`scripts/check-d1-behaviour.sh`) drives real HTTP
routes and the real `--test-scheduled` trigger against a scratch local
D1, in CI, on every push. Four subjects (07a, 07d, 07e, 08–10) each stood
this kind of harness up by hand, verified a behavioural claim once, and
threw it away — the harness never ran again, and the claim lived only in
an evidence log. This gate is the same instrument, kept.

**The boundary that makes this safe**: no route, no feature flag, no
`#[cfg]` added to either Worker (Option B,
`.git-exclude/reviewed/054-d1-ci-harness-proposal.md`). A feature-gated
`/__test/` surface was considered and rejected — a test surface reaching
a deployed Worker would be worse than G-21, and this project's own
history (G-21, G-32, G-33) is that a thing safe only because of a flag
nobody checks eventually ships. If a behaviour cannot be reached through
a real route or the scheduled trigger, it does not go in this gate — see
the script's own header for the one claim (DR-LIF-06, a retention pass
deleting only what it archived) currently excluded for exactly that
reason, and escalated rather than ported around.

It asserts on **responses and database state**, almost never on log
output — a log line describes behaviour, the row is the behaviour.
Currently covers: a re-import with `on_conflict=replace` does not
cascade-delete dependent rows (G-22); an unresolvable owner reference is
refused before any write, in both dry run and applied (G-31); an
imported target gets a `target_states` row and is actually probed by a
real scheduled tick (G-06); and — the one deliberate exception, added by
subject 07g — a scheduled tick makes exactly one of "retention ran" or
"the skip was logged, naming the minute" observable, never neither,
because that silent-neither outcome is the whole shape of the defect
(G-43) it closes. Each assertion is proven to fail when its own fix is
reverted — see the script's evidence log — not merely assumed to catch a
regression.

**`DR-LIF-06` (a retention pass deleting only what it archived) is still
not in this gate**, even after 07g closed G-43. The fix reads
`event.schedule()` instead of the wall clock, but under `wrangler dev
--local`, `event.schedule()` *is* the wall clock — its `--test-scheduled`
harness does not propagate a controllable nominal time through to the
compiled Worker, traced to workerd's own local-mode scheduled-event
simulation rather than this project's code or the `worker` crate (see
`.git-exclude/reviewed/058-subject-07g-escalation-ruling.md`). So this
gate can observe whichever of the two outcomes the real clock happens to
produce at run time, but still cannot drive a retention pass *on
demand*. That confirmation moves to `scripts/deployment-verify/
04-scheduled-event-time.md`, against a real Cron Trigger.

**A related instrument that answers plausibly while doing nothing**:
`/cdn-cgi/handler/scheduled`, used by subjects 08–10's evidence and an
earlier version of this gate's (c) assertion, returns `200 ok` but never
actually reaches the compiled Worker when called *with* query
parameters — confirmed during 07g's escalation. It only worked before
because every prior use called it bare (no `time=` parameter), which
happens to still reach the real handler at the real wall-clock time. The
documented endpoint, confirmed via `wrangler dev --help`, is
`/__scheduled` — use that, not `/cdn-cgi/handler/scheduled`, for any
future scheduled-trigger testing under `wrangler dev --local`.

### What is covered

The unit tests target pure logic — anything that can be exercised on the host x86_64 target without depending on `worker::*`, `js_sys::*`, D1, KV, or any other Cloudflare-specific runtime symbol. Specifically:

| Module | Tests | What they cover |
|---|---|---|
| `noye-shared::Caller` and `header::*` | 3 | Role check, JSON round-trip, header constant pinning |
| `core::db::states::decide_transition` | 10 | All state-transition rules from requirement 2-4 (up/down/unknown, threshold = 1, recovery progress, counter reset) |
| `core::db::incidents::calculate_duration` | 6 | Elapsed seconds, hour boundaries, invalid input, negative durations |
| `core::db::retention::compute_cutoff` | 5 | ISO-8601 UTC cutoff format, 0/1/N day windows |
| `core::db::channels::validate_endpoint` and `validate_channel_inputs` | 12 | HTTPS-only requirement for webhook/Slack, host-presence checks, single-`@` and dotted-domain rules for email, whitespace rejection, channel-type whitelist |
| `gateway::auth::crypto::base64url` | 6 | Round-trip, URL-safe alphabet, no-padding decode, error rejection |
| `gateway::auth::jwt::AudClaim::contains` | 7 | Single vs array `aud`, case sensitivity, deserialization from both forms |
| `gateway::auth::rbac` | 7 | `is_admin`, `is_owner`, `can_view_target`, full `has_permission` truth table for each role |
| `gateway::auth::turnstile` | 8 | siteverify form-body construction (with/without IP, URL-encoded chars), HTML escaping, response parsing for success/failure shapes |
| `gateway::ui::channels` | 10 | `mask_endpoint`: email local-part masking (`alice@…` → `a***@…`), URL host-only truncation, defensive fallback for malformed input. **Phase C**: `parse_retry_after` accepts integer-second values, trims whitespace, rejects negatives / non-numeric / HTTP-date forms (return `None` so callers fall back to "no specific hint"); `format_retry_after_hint` chooses the largest meaningful unit (seconds < 60 / minutes < 60 / hours), pluralises labels correctly ("1 second" vs "30 seconds", "1 minute" vs "1.5 minutes"), and clamps negative inputs to 0. The same logic is mirrored in inline JavaScript so the page works without server round-trips; the Rust copy exists to pin the wording in unit tests so the two implementations don't drift. |
| `gateway::ui::targets` (Phase C) | 21 | `TargetTab` enum with stable URL slugs (`overview` / `results` / `channels` / `settings`), distinct labels and slugs (uniqueness verified), `parse` round-trip, unknown / case-mismatched values fall back to Overview (defensive — malformed bookmarks still render); `all()` orders Overview first and Settings last. `protocol_help` returns specific help for `http`/`https`/`tcp`/`smtp`/`tls` and `None` for unknown types (page omits the help block entirely rather than guessing); each help string mentions the relevant concept (`expected_status`, `handshake`, `threshold`). `render_list` shows the management accordion only for admins, an empty-state card with `role="status"` when there are no targets, status badges + target-detail links for populated rows, `aria-disabled="true"` on disabled-target rows. `render_detail` renders a tab strip with `aria-current="page"` on exactly the active tab + a header card visible on every tab; Overview tab includes the protocol-help paragraph for known types and omits it for unknown; Results tab renders the OK/FAIL badge table when results are present and a friendly "no results yet" message when empty; Settings tab shows immutable metadata and links to `PUT /api/targets/:id`, omits the Tags row when unset; Channels tab renders a placeholder card (the actual attachment block is appended by the gateway handler when on the Channels tab). |
| `gateway::ui::maintenance` (Phase C) | 12 | `partition_windows` separates active from non-active preserving caller order; `format_scope` prefers `target_id` over `target_tag` (matching Core's apply-window logic) and falls back to "All targets" when neither is set, escapes HTML in inputs as defense-in-depth. `render_list` includes a help card explaining the four-bullet semantics ("notification suppression" — monitor keeps running, incidents still recorded, notifications suppressed, SLA denominator excluded); admin-only management accordion; active section uses the `maintenance` status badge; upcoming/past section uses a `scheduled` info-tone badge; friendly "no active windows" message when none are active; UTC timestamps emit `<time datetime>` with column headers explicitly labelled "Start (UTC)" / "End (UTC)" so the timezone is unambiguous; the "Notifications" column shows explicit `suppressed` / `unaffected` labels rather than the boolean string `true` / `false`; `created_by` is HTML-escaped as defense-in-depth. |
| `core::notify::format_*_message` | 5 | DOWN/UP/TEST message body shapes, error-message fallback when none is supplied, status field correctness for transport routing |
| `gateway::rate_limit::*` | 21 | Allow/deny truth table for per-minute and per-hour windows, off-by-one limits at exact threshold, both windows triggering simultaneously (per-minute takes precedence — shorter wait is surfaced first), `Retry-After` calculation at minute/hour boundaries, KV key isolation across channels and across scopes, bucket-id stability within a window. Login-specific tests: prefix isolation between `ratelimit:test:` and `ratelimit:login:` namespaces (no shared counters), IPv4 key shape, `ip_for_key` passes IPv4 unchanged, replaces `:` with `_` in IPv6, passes the literal `unknown` fallback through, and distinguishes distinct IPv6 addresses. |
| `core::stats::compute_sla` and `parse_window` | 18 | Window-format parser (`Nh`/`Nd` / rejection of unrecognized forms), zero-incident baseline, single resolved/open incident clipping at both window edges, overlapping incidents (union not sum), adjacent-incident merge, maintenance-overlap subtraction (full and partial), MTTR averaging across only resolved incidents, zero-length window edge case, malformed-timestamp tolerance, both ISO-Z and SQLite timestamp formats accepted |
| `gateway::ui::stats` formatters and window selector | 8 | Three-decimal-place uptime percentage formatting, duration formatting unit selection (s / m / h / d), defensive handling of negative duration input. **Phase B**: window selector renders as a `tabs` widget — active window has `aria-current="page"`, only one current marker per render, unknown window value yields no `aria-current`; emits no `<form>` or `<select>` (regression guard against the old shape). Per-target table includes a per-row CSV link to `/api/stats/incidents/:id.csv?window=...` carrying the current window, plus an interpretation hint distinguishing "Gross uptime" (counts every minute of downtime) from "SLA uptime" (excludes scheduled-maintenance time). |
| `gateway::ui::dashboard` (Phase B) | 15 | `metric_tone_for` maps the kind+value combination to a `MetricTone`: down>0 → red, open>0 → degraded, up>0 → green, all-zero or unknown kind → default; `targets_hint` formats "X up · Y down"; `select_open` filters to status=="open" preserving order; `render` produces a four-card metric strip (Targets / Up / Down / Open incidents) with appropriate tones, an open-incidents card showing only the open ones with target-detail links + a "View all incidents" link, a friendly "All clear" empty state, and an optional Status-breakdown card that is omitted entirely when degraded/maintenance/unknown/disabled are all zero (keeps the dashboard quiet on healthy systems); the metric strip's "Open incidents" count matches the body's filtered list. |
| `gateway::ui::incidents` (Phase B) | 26 | `partition_incidents` separates open from non-open preserving caller order, treating any non-`"open"` status as resolved (defensive for unknown codes); `ResolutionReason` enum with stable `code()` strings (`recovered_externally` / `transient` / `target_removed` / `other`), `from_code` round-trip, rejection of unknown / case-mismatched / empty codes, ordered `all()` with Other last (UI requires it last because the form unlocks the free-text only when the user picks Other); `compose_resolution_note` produces `[code] detail` when detail is non-empty / `[code]` when empty / strips surrounding whitespace / treats whitespace-only detail as empty; `format_duration` em-dash for None / `0s`-`59s` / `1m 0s`-`59m 59s` / `1h 0m`-`23h 59m` / `1d 0h`+. `render_list` empty state shows a friendly "no incidents" with `role="status"`; open card includes `role="status"` + count; resolved-only render omits the resolve-button column and dialog/script entirely (smaller markup when nothing to do); open-incidents render includes `data-incident-id` and `data-target-id` on each row's button; target_id renders as a link to the target detail; the resolve dialog lists every reason from `ResolutionReason::all()`; help card explains the notification-vs-incident split; open and resolved sections live in distinct `<section>` cards with stable IDs; cause text is HTML-escaped (XSS regression guard). |
| `gateway::ui::layout::contrast` | 9 | WCAG 2.1 contrast-ratio formula (white-on-black = 21:1 max, identical = 1:1 min, symmetry); hex parsing accepts canonical `#RRGGBB` and rejects malformed inputs (no `#`, 3-char shorthand, non-hex chars, wrong length); `meets_aa` distinguishes 4.5 (body) from 3.0 (UI) thresholds; **all 25 critical color pairs in the design tokens (text on each surface in both light and dark themes, every status badge fg/bg pair, primary button text on primary background) are pinned to WCAG AA** so any token edit that drops a pair below threshold fails the test before deploy. |
| `gateway::ui::layout::components` | 18 | `escape_html` covers all five HTML entities; `time_local` renders `<time datetime="...">` and escapes input as defense-in-depth; `BadgeKind::from_state` maps known status codes (up / down / degraded / maintenance / open / resolved / acknowledged) and falls back to `Unknown` for anything else; `status_badge` includes `role="status"` + `aria-label` and escapes the visible label; `status_badge_from_code` capitalises the visible label while reusing the same kind mapping; `card` with id_hint emits `aria-labelledby` linking to the heading id, without id_hint omits both; `card` escapes the title but passes the body through (caller-rendered HTML); `metric_card` renders label/value/optional hint and applies tone CSS classes (default / up / down / degraded); `tabs` marks exactly one active tab via `aria-current` and escapes both href and label; `inline_result` starts hidden + `aria-live="polite"` + `role="status"` and accepts four severity tones (success / error / warn / info) producing distinct CSS classes; `ButtonKind` produces four distinct CSS class strings. |
| `gateway::ui::layout` (`wrap` + `render_nav` + `render_user_info` + `active_route_for_title`) | 19 | `active_route_for_title` maps known page titles to nav routes (Dashboard, Targets, Incidents, Maintenance, Channels, Stats, Audit, Settings, Migration), handles detail-page prefixes (`Target: …` → `/targets`, `Channel: …` → `/channels`, `Stats: …` → `/stats`), returns `None` for chip-area pages (`Security`) and unknown titles (defensive — pages render but get no nav highlight); `render_nav` produces three verb-grouped columns labelled "Observe" / "Operate" / "Verify"; `render_nav` for member callers omits the Verify group entirely (no leaked admin-only links); active marker appears on exactly one nav link; absent active route produces zero `aria-current` attributes; `render_user_info` always renders Security and Logout links and the role badge, escapes the caller name; `wrap` emits skip link + `role="banner"` + `role="main"` + `role="contentinfo"` (semantic landmarks); CSRF meta tag present when token supplied, absent when `None`; admin sees Verify nav group, member does not; page title is HTML-escaped; body content passes through unescaped (caller is responsible for escaping). |
| `core::notify::email` | 16 | Email-shape validation (canonical / common-mistake forms / oversized rejection); `TlsMode::for_port` (465 → Implicit, others → STARTTLS); `TlsMode::parse` recognizes `implicit`/`smtps`/`tls`/`starttls` (case-insensitive) and rejects `none`/`plain`/empty; `build_message` (mail-builder factory) emits a complete RFC 5322 message — From with display name, From with empty name (no `"" <addr>` leak), To/Subject/Message-ID/Date headers all present, explicit Message-ID (overriding the default-feature-disabled `gethostname` path), `X-Mailer: noye-monitoring` header, blank-line CRLF separator between headers and body, body content passthrough across multi-line input, and `=?utf-8?B?...?=` / `=?utf-8?Q?...?=` framing for non-ASCII subjects; `status_message` mentions `EMAIL_SMTP_HOST` for `Disabled` and includes the misconfiguration reason verbatim. |
| `core::db::audit::hash::*` | 21 | Canonical row encoding: starts with version tag, separates 11 fields with `\x1F`, treats `None` / `Some("")` identically, distinguishes distinct field values and distinct timestamps. SHA-256 row hash: deterministic across calls, emits 64 lower-case hex chars, changes when prev_hash changes, changes when any field changes (action_type, actor_id), genesis hash is 64 zeros, known-value pin for hash stability. Verification: accepts the correct (stored, prev, row) triple, rejects tampered row, rejects swapped prev_hash. Well-formedness: accepts genesis and real outputs, rejects wrong length / uppercase / non-hex chars. End-to-end chain: three rows where each `row_hash` depends on the prior, every step verifies, and a chain break is detected at the tampered row. |
| `core::migration::validate` and `count_rows` | 17 | Schema-version mismatch fails immediately, duplicate-ID detection across each table, foreign-reference checks (`target_notifications` referencing unknown target_id or channel_id, duplicate (target, channel) pairs), owner-integrity warnings (only when users are exported), per-record sanity (empty IDs / unknown channel_type), error accumulation rather than short-circuiting, well-formed payloads validate clean, `ImportConflictPolicy` defaults to Skip and round-trips through serde with lowercase variants, row-count helper returns each table size including absent users |
| `gateway::csv_export::*` | 19 | RFC 4180 field quoting (commas, embedded double quotes doubled, newlines, CRs), four-decimal-place ratio formatting, plain-integer second formatting (incl. defensive negative input), CRLF row terminators, BOM presence on every output, `encode_sla_summary` row-per-target structure, empty / `None` MTTR cell, comma-in-target-name quoting, Unicode pass-through, `encode_incidents` resolved/open shape, embedded-quote causes, filename construction |
| `gateway::security_headers::*` | 8 | Policy completeness check (CSP, HSTS, X-Frame-Options, X-Content-Type-Options, Referrer-Policy, Permissions-Policy all present); CSP includes `frame-ancestors 'none'` + `object-src 'none'` + `base-uri 'self'`; CSP `default-src 'self'`; X-Frame-Options=DENY; X-Content-Type-Options=nosniff; HSTS at least 1 year + includeSubDomains; Referrer-Policy is no-referrer; Permissions-Policy denies camera/microphone/geolocation/payment. |
| `gateway::safe_redirect::*` | 11 | `is_safe_return_to` accepts `/`, simple paths, paths with query and fragment; rejects empty, absolute URLs (`https://`, `javascript:`, `data:`), protocol-relative URLs (`//evil`), backslash tricks (`/\evil`), CR/LF injection, relative paths without leading slash. `sanitize_return_to` returns input unchanged when safe and falls back to `/` for any unsafe input. |
| `gateway::env_check::*` | 12 | `Environment::parse` recognizes "development" case-insensitively; treats unset / unknown / "dev" / "staging" as production (fail-safe default); `KNOWN_DEV_FALLBACKS` lists both the OIDC client secret and the Gateway shared token, matching the values documented in `wrangler.toml.example`'s instructional comments (drift fails this test) — neither value ships in a file this test can read at build time, since the template carries no secrets at all. `find_leaked_fallback` (pure, Subject 03 / G-21): refuses a denylisted value with no `NOYE_ENV` parameter at all — there is no development bypass left to regress into; accepts unset / non-denylisted values; error names the variable, never the value; only the leaking variable is named when just one of several leaks. |
| `core::env_check::*` | 5 | Same denylist semantics as the gateway, scoped to `GATEWAY_SHARED_TOKEN` only. `Environment` was removed entirely from this crate (Subject 03) — Core has no cookie/session concerns that ever consumed it, so keeping it after the fallback check stopped branching on it would have been dead code. |
| `gateway::auth::cookie::*` | 13 | Cookie builder default attributes (Secure / HttpOnly / SameSite=Lax / Path=/); Max-Age inclusion when set; expired cookies use Max-Age=0; `secure(false)` drops the attribute; `secure(true)` keeps it; `expired().secure(false)` for development logout. Cookie header parser handles named values, surrounding whitespace, missing keys, empty headers, base64-style values with `=` padding, duplicate-name first-match. |
| `gateway::auth::csrf::*` | 10 | `constant_time_eq` matches identical strings (incl. empty and full 43-char tokens), rejects distinct strings (whole-string and first-byte and last-byte mismatches — guards against accidental short-circuit), rejects different lengths. `looks_well_formed` accepts 43 chars of base64url (full alphabet incl. `_-`), rejects wrong length (0 / 3 / 42 / 44), rejects standard-base64 padding chars (`=`, `/`, `+`), rejects whitespace. |
| `gateway::auth::session::ids_to_revoke_excluding_current` | 5 | Excludes only the current session id from the revoke set; returns every input session when current id is absent (so an unknown current-session value does not preserve it inadvertently); empty input returns empty; only-current input returns empty; preserves input order across the filter (stable for diagnosis). |
| `gateway::ui::me::format_unix_ts` | 4 | Returns `-` for the placeholder zero value (never-set issued/expires fields), formats a known unix timestamp as `YYYY-MM-DD HH:MM:SS UTC`, handles unix epoch +1 second, handles 2100-era timestamps without overflow. |
| `gateway::ui::audit` (Phase D) | 15 | `action_label` returns stable `("create"/"update"/"delete"/"login"/"import"/"resolve", same)` for known buckets and `("other","other")` for unknown — kept as a `pub` API for future label-based aggregations even though the row renderer surfaces the raw `action_type`. `render_list` empty state has `role="status"` and explains what mutations populate the log; the intro card links to `/me/security` for hash-chain integrity verification rather than duplicating the button; success/failure rows use `badge-up` / `badge-down`; the actor cell prefers `actor_email` over `actor_id` and falls back to id when email is None; the "Changes" cell uses `<details>` with prev/new value `<pre>` blocks when either field is present and a plain em-dash when both are absent (no disclosure widget for nothing to disclose); unknown action types render the raw value verbatim in the badge so future events surface correctly; `time_local` is used for action_time; missing resource_id and ip_address render as em-dashes; actor_email is HTML-escaped (XSS regression guard). |
| `gateway::ui::settings` (Phase D) | 11 | Empty state explains how the first user gets registered (auto-OIDC mapping or this form) and includes the upsert form for manual registration. Admin/member role badges use distinct CSS classes (`badge-maint` for admin, `badge-info` for member) with `aria-label` attributes for screen readers. Active/inactive status badges use `badge-up`/`badge-unknown` and pair with descriptive `aria-label`. The upsert form covers all four `ManageUserInput` fields (email / name / role / is_active) with both role values present in the `<select>`. Form help text explicitly explains the deactivation-vs-deletion semantics ("audit log preserves the actor_id"). The Phase A `inline_result` panel is wired to `id="user-form-result"` with `aria-live="polite"`. The system-info card lists OIDC + D1 + KV + R2 components. Migration card links to `/admin/migration`. User name and email are HTML-escaped. |
| `gateway::ui::migration` (Phase D) | 7 | Members see a restricted-access message; admins see all four sections (intro / export / import / bulk pointer) each with stable IDs. Export form has the include_users checkbox off by default and surfaces the "Off by default" wording in the help text so the markup default and the visible text stay in sync. Import form's default conflict policy is "skip" (carries `checked` attribute on the radio). Apply checkbox unchecked by default means clicking "Run" is a dry-run by default (the help text says so explicitly). Phase A `inline_result` panels with `aria-live="polite"` for both export and import. **Regression guard**: the rendered HTML must not contain references to the deprecated `--color-fg-muted`, `--color-success`, or `--color-danger` token names that were dropped in Phase A; the test fails if any of them reappear, preventing a future contributor from accidentally restoring stale token references. |
| **CLI:** `noye` (host bin) | 7 | Email validation: accepts canonical and `+` forms; rejects empty / no `@` / oversized (RFC 5321 254-byte limit) / SQL-dangerous characters (single quote, double quote, semicolon). SQL escaping: doubles single quotes, passes through safe strings unchanged. |
| **Dev IdP:** `noye-dev-idp` (host bin) | 10 | RS256 JWT structure (3 dot-separated parts; correct header alg/typ/kid; payload carries iss/sub/aud/nonce/email/email_verified; exp is now+lifetime within tolerance). In-memory authorization-code store (single-shot consume; unknown code returns `None`; codes older than 60s expire). URL-encoded query parsing (basic key=value pairs, percent-encoded values, empty value, empty input). |
| **WASM target:** `gateway::auth::crypto::*` (digest, random, base64url, jwt_verify) | 12 | SHA-256 against FIPS 180-4 vectors (empty input, "abc" example 1, FIPS 180-4 example 2, output-length invariants); cryptographic RNG length / distinctness / non-zero properties; base64url round-trip under WASM; RS256 signature verification using RFC 7515 §A.2 vectors (valid signature accepted, tampered payload rejected, tampered signature rejected, unsupported alg rejected, wrong key rejected) |

### What is not covered (and why)

Functions that touch `worker::Request`, `worker::D1Database`, `worker::Env`, or D1/KV/Service Bindings cannot be unit-tested in either target because the bindings are only injected by the Workers runtime at fetch time. Examples:

- `auth::extract_caller` (touches `worker::Request` and calls into the Core)
- `auth::session::create` / `load_from_cookie` / `destroy` (KV)
- `auth::oidc::*` (fetch — JWKS download and token exchange — beyond what the WASM tests cover, which is signature verification only)
- Every `db::*` function below the pure helpers (D1)
- Every `monitor::*` checker (fetch / TCP `connect`)
- Every `notify::*` channel transport (fetch / SMTP)

The Web Crypto / `js_sys` boundary that previously appeared in this list is now covered by the WASM-target tests above. A handful of specific `db::migration`/`db::states` behaviours are now covered a different way — not as unit tests, but as CI-driven assertions against real local D1 (see "A behaviour gate, not just a boundary check" above). That is deliberately narrow: it exercises what a subject's evidence log claimed, not the module generally, and every `db::*` function not named there is still unreachable from any automated test. The remaining items would need either a Miniflare harness (TypeScript-side) or `worker::testing` (not yet stable in the `worker` crate at 0.8) — see `requirements.md` Roadmap for the deferred-work record.

The strategy is to push as much logic as possible into pure helpers (e.g. `decide_transition`, `compute_cutoff`, `parse_cookie_header`, `compute_sla`) and call them from the I/O-bound wrappers. New code should follow the same pattern.

### Adding a test

For a pure function (no JS bindings), add a host test inside the same file at the bottom:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptive_snake_case_name() {
        assert_eq!(my_function(input), expected);
    }
}
```

For a function that depends on `globalThis.crypto.*` or other JS bindings, write a WASM test in a separate `mod wasm_tests` block in the same file:

```rust
#[cfg(all(test, target_arch = "wasm32"))]
mod wasm_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    #[wasm_bindgen_test]
    async fn descriptive_name() {
        let result = my_async_jsbinding_function().await.unwrap();
        assert_eq!(result, expected);
    }
}
```

Run `cargo test --workspace --lib --bins` for the host suite and `cargo test -p noye-gateway --target wasm32-unknown-unknown` for the WASM suite before committing. Both must pass. Failures in any crate or target fail the workspace test pass.

## Continuous integration

The repository includes a GitHub Actions workflow at `.github/workflows/ci.yml` that runs on every push, every pull request, and on a weekly cron (Saturdays 02:00 UTC). The workflow has four jobs.

### `check`

Runs `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo check --workspace`. The `RUSTFLAGS=-D warnings` environment lifts every Rust-level warning to a hard error, so any unused-import or dead-code regression breaks the build.

### `test`

Runs `cargo test --workspace --lib --bins --locked` against the host target. This is the same command contributors use locally, plus `--locked`. As of 0.27.0 the suite is 435 tests across 5 crates.

WASM-target tests (the `auth::crypto::*` modules under `wasm32-unknown-unknown`) are not yet wired up in CI — they require `wasm-bindgen-test-runner` and a headless browser, which we have not set up in the workflow. The `wasm-build` job below at least verifies that the same code compiles for the WASM target.

### `wasm-build`

Runs `cargo check --target wasm32-unknown-unknown` for `noye-gateway` and `noye-core`. This catches non-portable dependencies (e.g. one whose `getrandom` backend is incompatible with `wasm32-unknown-unknown`) before they reach `wrangler deploy`.

### `audit`

Runs `cargo audit` to scan the dependency graph against the [RUSTSEC advisory database](https://rustsec.org). The job is `continue-on-error` on pull requests (so a freshly-disclosed advisory doesn't block an unrelated PR) but blocking on push to `main` and the weekly cron run.

### Lockfile policy

`Cargo.lock` is **committed to the repository**. CI uses `--locked` so a lockfile-drift causes a hard failure rather than a silent dependency upgrade. To intentionally upgrade a dependency, run `cargo update <crate>` and commit the updated `Cargo.lock` as part of the change.

### Toolchain

CI pins Rust 1.91 (the minimum required by `resolver = "3"` and Edition 2024). Local development should use the same version; `rustup toolchain install 1.91 && rustup default 1.91` keeps things in step.
