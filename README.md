# Noye

[![CI](https://github.com/nabbisen/noye/actions/workflows/ci.yml/badge.svg)](https://github.com/nabbisen/noye/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)
[![Rust 2024](https://img.shields.io/badge/rust-2024-orange.svg)](Cargo.toml)

**Lightweight server health-monitoring on Cloudflare Workers — minimal,
auditable, accessible by default.**

---

## Overview

Noye watches a small set of endpoints (HTTP / HTTPS / TCP / SMTP / TLS
certificates), records every state transition, and dispatches notifications
through your choice of webhook, Slack, or email. It runs entirely on the
Cloudflare edge — D1 for state, KV for sessions, R2 for archival, Cron
Triggers for scheduling — and ships with an OIDC-authenticated web UI
that is server-rendered, keyboard-navigable, and contrast-checked at
compile time.

## Why Noye

Noye is for small teams who want **explicit, low-magic monitoring** they
can read end-to-end and operate confidently:

- **You watch tens, not thousands, of endpoints.** Noye targets the
  small-fleet end of the curve. Nagios and Prometheus stack are great
  but heavy; Noye is closer to "a Cron job and a database, written
  carefully."
- **You care about transparency.** Every mutation is appended to a
  hash-chained audit log. Notification suppression windows are
  explicit. The whole code base fits in five small Rust crates.
- **You care about accessibility.** The UI is built ABDD-first
  (Accessible by Default and by Design). It works without JavaScript,
  meets WCAG 2.1 AA contrast, and the contrast is verified by unit
  tests so a token edit can't silently regress it.
- **You're already on Cloudflare** — or are willing to be — and you'd
  like to keep your monitoring on the same plane as everything else.

If your scale is "hundreds of services across multiple teams with
on-call rotations," reach for Prometheus + Alertmanager + Grafana
instead. Noye is an opinionated tool for the smaller end.

## Quick Start

You can run the entire system locally with no Cloudflare account and no
external IdP. Three terminals for the running services, plus one CLI
command to seed an admin user.

### Prerequisites

```bash
rustup target add wasm32-unknown-unknown
cargo install worker-build
npm install -g wrangler        # v4
```

### Run

```bash
# Terminal 1 — Local OIDC stub on port 5556
cargo run -p noye-dev-idp

# Terminal 2 — Core (D1, R2, Cron) on port 8788
cd crates/core
wrangler dev --port 8788

# Terminal 3 — Gateway (HTTP, sessions, UI) on port 8787
cd crates/gateway
wrangler dev --port 8787

# Terminal 4 (one-time) — Seed the local admin user
cargo run -p noye -- admin create \
  --email admin@local.test --name "Local Admin"
```

Open <http://localhost:8787>. Sign-in proceeds through the local OIDC
stub and you reach the dashboard immediately. The dev IdP serves a
single hard-coded user and signs ID Tokens with a freshly generated
RSA-2048 key on every restart — nothing leaves your machine.

### Run the tests

```bash
cargo test --workspace --lib --bins
```

No Wrangler or dev-idp instance required.

### Going further

For deeper local-development workflows (running individual crates,
WASM-target tests, contributor tooling) see
[docs/src/development.md](docs/src/development.md). For the local OIDC
stub's configuration and behaviour, see
[docs/src/dev-idp.md](docs/src/dev-idp.md).

When you're ready to put Noye on Cloudflare (provisioning D1 / KV / R2,
configuring a real OIDC client, managing secrets, deploying), see
[docs/src/setup.md](docs/src/setup.md).

## Design Notes

Noye follows two non-negotiable principles:

1. **Unix philosophy: minimum features for safety and transparency.**
   The system implements only what the requirements call for and
   nothing more. "Useful but out of scope" is a real category and the
   [ROADMAP.md](ROADMAP.md) exists for it.
2. **ABDD — Accessible by Default and by Design.** Every web page is
   server-rendered semantic HTML, navigable by keyboard, readable
   without CSS or JavaScript, and contrast-checked at compile time
   against WCAG AA. ABDD is a baseline, not a polish step.

Two small architectural choices flow from these principles:

- **Two-Worker split with no public Core.** The Gateway terminates
  external HTTPS, runs OIDC, manages sessions, and renders all UI; it
  has no D1 binding. The Core owns D1 / R2 / Cron and is reachable
  *only* via Service Bindings — `workers_dev = false`, no custom
  route. Public traffic cannot land on it directly.
- **Pure-function UI helpers.** Pages compose by stitching pure
  functions that return `String`. No template engine, no DOM
  manipulation, no client framework. Every helper is unit-testable on
  the host target without spinning up a Worker runtime.

For the full layered architecture, security model, and design
rationale, see [docs/src/architecture.md](docs/src/architecture.md) and
[docs/src/notes.md](docs/src/notes.md).

## Documentation

Full documentation lives under [docs/src/](docs/src/) and is structured
by reader persona — first-time users, operators, and maintainers.
The [SUMMARY.md](docs/src/SUMMARY.md) is the table of contents; if you
build it locally with [mdBook](https://rust-lang.github.io/mdBook/) it
becomes a navigable web book:

```bash
cargo install mdbook
mdbook serve docs
```

Key chapters:

- [Introduction](docs/src/introduction.md) — what Noye is, in detail
- [Why Noye?](docs/src/why.md) — when to choose it, when not to
- [Architecture](docs/src/architecture.md) — layers, boundaries, threat model
- [API reference](docs/src/api.md) — Gateway external API, Core internal API
- [Setup on Cloudflare](docs/src/setup.md) — provisioning + first deploy
- [Operations](docs/src/deployment.md) — rollouts, rollbacks, troubleshooting
- [Accessibility (ABDD)](docs/src/accessibility.md) — what we mean and how it's enforced
- [Security posture](docs/src/security-posture.md) — controls, threat model, operator checklist
- [Development guide](docs/src/development.md) — for contributors and maintainers
