# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Note: changelog entries begin with the next release. Earlier history is
recorded in the project's release tags and the [docs/src/requirements.md](docs/src/requirements.md)
coverage table.

## [Unreleased]

### Added

### Changed

### Fixed

### Removed

### Security

## [0.27.2] — 2026-05-04

A documentation-only release. No code changes; no behavioural change.
This release introduces the `rfcs/` directory carrying detailed
specifications for the priority items deferred in `ROADMAP.md`, so an
implementer picking up any of them does not have to reconstruct the
design choices.

### Added

- `rfcs/` directory at the workspace root with:
  - `rfcs/README.md` — index, workflow contract, and the rule for
    when an RFC graduates from `proposed` to `shipped`.
  - Six priority-item RFCs (`001`–`006`) covering: manual theme
    toggle, audit-log mirror via Cloudflare Logs, Turnstile activation
    on `/auth/login`, failed-login audit recording, high-contrast
    theme preset, and Slack-specific notification payload formatting.

### Changed

- `ROADMAP.md` — each entry that has a corresponding RFC now links to
  it so the high-level roadmap and the implementation-ready spec are
  reachable from each other.

## [0.27.1] — 2026-05-03

A documentation-and-tooling release. No code changes; no behavioural
change in production. The release records that the supply chain was
audited at this point and that one upstream advisory has been
explicitly evaluated and suppressed with reasoning.

### Added

- `.cargo/audit.toml` — `cargo-audit` ignore configuration carrying a
  documented suppression for `RUSTSEC-2023-0071` ("Marvin Attack" in
  the `rsa` crate). The entry includes the threat-model rationale
  inline so a future contributor reading `cargo audit` output can
  follow the chain back to the decision.

### Security

- Full RUSTSEC scan of the 0.27.0 lockfile (223 unique
  `(name, version)` pairs) recorded as a release artifact:
  - 0 confirmed CVE exposures
  - 0 unmaintained / informational notices
  - 1 documented suppression (`RUSTSEC-2023-0071`, see below)
- Documented `rsa` 0.9.10 (`RUSTSEC-2023-0071` / CVE-2023-49092)
  as a known-and-evaluated finding. The crate is reachable only via
  `noye-dev-idp`, a local-development OIDC stub that binds to
  `localhost` and is never deployed; the Marvin Attack threat model
  does not apply. See [`docs/src/security-posture.md`](docs/src/security-posture.md)
  for the full assessment, including the criteria for revisiting the
  decision.
