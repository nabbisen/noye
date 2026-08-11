# Noye — Software Requirements Specification

**Baseline version:** v0.27.2, amended 2026-07-28 for v0.28.0
**Language:** English (project working language)

**Amendment note (2026-07-28).** This document supersedes the former
requirements-traceability matrix that occupied this path. Amendments
approved on 2026-07-28: decisions D-1 and the role model closed (§13);
DR-LIF-06 reworded; DR-LIF-07, DR-MIG-05, NFR-QA-10, NFR-SEC-14 and
NFR-SEC-15 added; eight statuses corrected against verified evidence
(§11.1); gaps G-19…G-29 added. Identifiers are never reused.

**Further amendment (2026-07-28, same date).** Decision D-2 closed:
suppression windows split "silence notifications" and "exclude from
SLA" into two independent flags (DEC-013). FR-SUP-03 restated to an
exclusivity rule rather than a precedence rule; FR-SUP-11 restated so
its fourth semantic is conditional; FR-SUP-13 and FR-SLA-09 added.

**Further amendment (2026-07-28, Subject 01 closed).** G-01 struck —
`sql/0002_audit_hash_chain.sql` deleted per DEC-010; DR-MIG-01,
DR-MIG-05, and NFR-QA-10 move to `Implemented`. DR-MIG-02 reworded
(review reply `012-reply-subjects-01-02.md`): it now speaks to a
migration's *applied effect*, not its bytes, so the comment-only edit
Subject 01 made to an already-released `sql/0001_initial.sql` satisfies
it as reworded rather than merely being tolerated. DR-MIG-02's status
otherwise remains `Not met`, unrelated to this edit — see the DDL
amendment that actually caused G-01.

**Further amendment (2026-07-28, Subject 02 closed).** G-20 struck —
`crates/core/src/db/retention.rs` restructured into a per-batch
archive-then-delete loop, plus Build step 4 (`requires_archival`):
`archive_to_r2 = 0` is refused as a configuration error for
`check_results` and `incidents`, rather than honoured into an unarchived
delete. DR-LIF-06 moves to `Implemented`; DR-LIF-07 moves to `Partial`
(guaranteed by code structure, not exercised against live D1/R2 fault
injection — no such environment was available while implementing this).
Retention batch size recorded as DEC-017, pending live verification
(subject 36).

**Further amendment (2026-07-28, Subject 03 built).** G-21 struck —
both `wrangler.toml` files renamed to `.example` templates carrying no
secret values and `NOYE_ENV = "production"`; the dev-fallback denylist
check in both `env_check.rs` files no longer branches on environment at
all. NFR-SEC-09, NFR-SEC-14, NFR-SEC-15 move to `Implemented`. Core's
now-unused `Environment` type was removed rather than kept dead. Audit
found one further finding (F-3: Core's template still set the
now-unused `NOYE_ENV`) and, independently while capturing release
evidence, one process defect unrelated to Subject 03 (see next
amendment) — M0 was not yet complete at this point; see below.

**Further amendment (2026-07-28, F-3/F-4/Subject 03a closed — M0
complete).** F-3 closed: Core's template no longer sets `NOYE_ENV`; six
other places that had absorbed the same wrong premise (both Workers
read it) corrected alongside. G-32 struck (new gap, found capturing
`cargo audit` evidence for this release, not caused by Subjects 00–03):
`.github/workflows/ci.yml`'s audit job ran `cargo audit --locked`,
which cargo-audit 0.22.2 rejects — the dependency-scan gate had not
actually run since it was introduced, corroborated by RUSTSEC-2026-0190
(published 2026-06-25) going undetected until run by hand on
2026-07-28. NFR-SEC-10 moves to `Partial` (fix applied, unconfirmed by
a live Actions run — this environment cannot trigger one). G-24's
archive-layout half struck (Subject 03a, pulled forward from Subject 34
because releases begin at M0): `package.sh`'s `--transform` removed;
PRQ-08 moves to `Implemented`. G-24's language half (CON-09) stays open
under Subject 34. This closes M0 (v0.28.0) — Subjects 00, 01, 02, 03,
and 03a are all done, pending the maintainer's decision on tagging and
packaging.

---

## 1. About this document

### 1.1 Purpose

This document states **what Noye must do and must be**, independently of
how it is currently built. It is the reference a reviewer uses to decide
whether an implementation is correct, and the reference an implementer
uses when a design question has no obvious answer.

It deliberately separates three things that are easy to conflate:

- the **requirement** (what must hold),
- the **acceptance criterion** (how you check it holds),
- the **current status** (whether it holds today, in v0.27.2).

Where the implementation does not currently satisfy a requirement, this
document says so rather than quietly restating the implementation as if
it were the specification. Those cases are collected in
[§11 Conformance gaps](#11-conformance-gaps).

### 1.2 Audience

| Reader | What to read first |
|---|---|
| New implementer | §2 overview, §3 glossary, then the functional area you are touching |
| Reviewer | §5–§8 requirement tables, §9 traceability |
| Operator | §7.2 security, §8 compatibility, §6.4 retention |
| Maintainer / architect | §11 gaps, §12 deferred scope, §13 open decisions |

### 1.3 Source documents

This specification consolidates and supersedes the following drafts.
Where they disagree, **this document is authoritative** and the
divergence is noted inline.

| Source | Contribution | Notes |
|---|---|---|
| `要件書 v1` (business/functional requirements draft) | Original business requirements, per-protocol "normal" definitions | Assumed Cloudflare Access and Leptos; both superseded |
| `開発指示書 v1` (development instruction) | Philosophy (Unix minimalism, ABDD), storage split, scheduler rule | Philosophy retained verbatim |
| `開発指示書 v2 (v0.27.1)` | Reconciliation of v1 against the shipped system | Primary source for §5–§7 |
| `functional-spec v0.22.0` / `v0.26.0` | Screen-level and API-level behaviour | Primary source for §5.13 UI |
| `GUI design v2 (v0.27.1)` | Route table, RBAC visibility matrix, component semantics, responsive rules | Frontmatter still names Leptos SSR; superseded by pure-function UI |
| `データ構造レビュー結果 (v0.27.1)` | Data-model defects and constraint gaps | Findings independently re-verified against v0.27.2 source for this document |
| `現状整理 (v0.27.1)` | Confirmed decisions, implicit assumptions, open questions | Primary source for §13 |
| Project development instructions | Process rules, i18n mandate, release packaging, documentation structure | Primary source for §10 |

### 1.4 Requirement conventions

Requirement keywords follow RFC 2119:

- **MUST / MUST NOT** — absolute. A violation is a defect.
- **SHOULD / SHOULD NOT** — strong recommendation. Deviation requires a
  recorded rationale.
- **MAY** — genuinely optional.

Identifier scheme:

| Prefix | Meaning |
|---|---|
| `FR-<area>-<nn>` | Functional requirement |
| `DR-<area>-<nn>` | Data requirement |
| `NFR-<area>-<nn>` | Non-functional requirement |
| `CON-<nn>` | Environment / compatibility constraint |
| `PRQ-<nn>` | Development-process requirement |

Identifiers are **stable**. A withdrawn requirement keeps its number and
is marked `Withdrawn`; numbers are never reused.

### 1.5 Status vocabulary

| Status | Meaning |
|---|---|
| `Implemented` | Satisfied in v0.27.2; verified against source or tests |
| `Partial` | Implemented, but with a known deviation recorded in §11 |
| `Not met` | Required, but absent or actively contradicted by the implementation |
| `Deferred` | Accepted requirement, scheduled via `rfcs/` or `ROADMAP.md` |
| `Decision required` | Cannot be assigned a status until a product decision is made (§13) |

Statuses in this document were established by reading the v0.27.2
source tree and by re-verifying each data-model finding directly; they
are not carried over on trust from the review drafts.

---

## 2. Product overview

### 2.1 Mission

Noye periodically probes a set of network endpoints, decides whether
each one is healthy, records every state change with an auditable
trail, and notifies operators when health changes. It runs entirely on
Cloudflare's edge platform.

### 2.2 Positioning and intended scale

Noye targets the **small-fleet** end of monitoring: tens to a few
hundred endpoints, operated by a handful of people. The design premise
is that such a team is better served by a system it can read end to end
than by a general-purpose observability stack.

Above roughly one thousand targets, or where multi-team on-call routing
and metric collection are needed, Prometheus + Alertmanager is the
correct tool and Noye is not.

### 2.3 Design philosophy

Two principles constrain every requirement in this document. They are
unchanged since project inception and are not open to incremental
erosion.

**P-1 — Unix philosophy: minimum features for safety and transparency.**
Only what the requirements call for is implemented. "Useful but out of
scope" is a real and frequently-used category; its destination is
`ROADMAP.md` and `rfcs/`, not the codebase.

**P-2 — ABDD: Accessible by Default and by Design.**
Every page is server-rendered semantic HTML, operable by keyboard,
readable without CSS or JavaScript, and contrast-verified at compile
time. Accessibility is a baseline property, not a later polish pass.

A third principle governs presentation:

**P-3 — Less is more.**
Sophistication comes from restricting what is shown and from
considered workflows. Excessive data on one screen is noise, especially
for a new operator. Advanced views may be offered *in addition to*, never
*instead of*, a quiet default.

### 2.4 Non-goals

The following are explicitly **out of scope** and MUST NOT be added
without a superseding decision recorded in `rfcs/`:

| Non-goal | Reason |
|---|---|
| Metric collection (CPU, memory, disk) | Noye is an availability monitor, not a metrics platform |
| Log aggregation and search | Same |
| APM / distributed tracing | Same |
| Client-side application framework | Conflicts with P-2 (no-JS operability) |
| On-call rotation / paging escalation | Out of the small-team premise |
| Portability to non-Cloudflare runtimes | D1 / KV / R2 / Cron Triggers are assumed throughout |

---

## 3. Glossary and terminology rules

Terminology is normative: the UI, the documentation, and this
specification MUST use these terms consistently, because several were
chosen specifically to prevent operator misreadings.

| Term | Definition | Rule |
|---|---|---|
| **Target** | One monitored endpoint with its probe configuration | — |
| **Check** | A single probe attempt against a target | — |
| **Check result** | The recorded outcome of one check | — |
| **State** | A target's current health: `up`, `down`, or `unknown` | — |
| **Transition** | A change of state, after threshold logic | Notifications fire only here |
| **Incident** | The interval from a `down` transition to the matching recovery | — |
| **Notification suppression** | A scheduled window in which checks continue and incidents are recorded, but no notification is dispatched | MUST be used in all user-facing text. "Maintenance window" is an internal/schema term only |
| **Channel** | A notification destination (webhook, Slack, email) | — |
| **Attachment** | The link between a target and a channel, with `on_down` / `on_up` flags | — |
| **Gross uptime** | Uptime counting all downtime | — |
| **SLA uptime** | Uptime with suppression-window time excluded | See §5.9 for the exact formula |
| **Gateway** | The public-facing Worker | — |
| **Core** | The private Worker, reachable only via Service Binding | — |
| **Operator** | A human using the web UI | — |

Additional naming rules:

- Incident states are named **Open** and **Resolved**. "Active",
  "Closed", and "Pending" MUST NOT be used.
- The admin navigation group is labelled **Verify**, not "Admin" — the
  three navigation groups are named by the operator's verb.
- Error and warning copy MUST be calm. Factual severity is acceptable
  ("Tampering detected"); alarm decoration is not ("🚨 ERROR!").

---

## 4. Stakeholders and roles

| Role | Authentication | Capability summary |
|---|---|---|
| **Anonymous** | None | `/healthz` only |
| **Member** | OIDC | Read-only, and only for targets they own |
| **Admin** | OIDC | Full read/write across all resources, user administration, audit verification, configuration migration |
| **System** | None (internal) | The Cron monitor acting on its own behalf; recorded in the audit trail as actor `system` |
| **Operator (deployment)** | Cloudflare account | Provisions bindings, sets secrets, deploys, configures the IdP |

There is no guest or self-registration role. An OIDC subject with no
matching `users` row is refused.

---

## 5. Functional requirements

### 5.1 Authentication and session (`FR-AUTH`)

Noye authenticates operators through a generic OpenID Connect provider.
The original requirement specified Cloudflare Access as a mandatory
front door; this was superseded to remove vendor lock-in on the identity
layer. Cloudflare Access MAY still be placed in front of the Gateway,
but MUST NOT be required for correct operation.

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-AUTH-01 | The system MUST authenticate operators using OIDC Authorization Code flow with PKCE, `state`, and `nonce`. | An authorization request carries `code_challenge`, `state`, and `nonce`; a callback with a mismatched `state` or `nonce` is rejected. | Implemented |
| FR-AUTH-02 | The system MUST work with any standards-conformant OIDC provider **that publishes a discovery document**, with no provider-specific code paths. | Google, Entra ID, Auth0, Okta, Keycloak, GitLab, and the bundled dev stub all authenticate using only configuration. | Partial — a provider without discovery is unsupported until FR-AUTH-03 is met |
| FR-AUTH-03 | Endpoint discovery MUST be supported, with per-endpoint overrides available for providers lacking discovery. | `OIDC_AUTH_URL` / `OIDC_TOKEN_URL` / `OIDC_JWKS_URL` override discovery when set. | **Not met** — no override variables exist; endpoint resolution is discovery-only (§11 G-19) |
| FR-AUTH-04 | ID token signatures MUST be verified against the provider's JWKS before any claim is trusted. | A token signed by an unknown key is rejected. | Implemented |
| FR-AUTH-05 | Sessions MUST be server-side, keyed by an unguessable identifier of at least 256 bits of entropy. | Session identifier is 32 random bytes, base64url-encoded. | Implemented |
| FR-AUTH-06 | The session cookie MUST be `HttpOnly`, MUST be `SameSite=Lax`, and MUST carry `Secure` whenever the environment is not `development`. | Response `Set-Cookie` inspected in both environments. | Implemented |
| FR-AUTH-07 | Session lifetime MUST be bounded and configurable, defaulting to 8 hours. | `SESSION_DURATION_MINUTES` defaults to 480; an expired session is refused. | Implemented |
| FR-AUTH-08 | A user MUST be able to enumerate their own active sessions and revoke all sessions other than the current one. | `/me/security` lists sessions; "Log out of all other sessions" invalidates them. | Implemented |
| FR-AUTH-09 | Logout MUST be reachable by a plain link (no JavaScript) and MUST invalidate the server-side session. | `GET /auth/logout` works with scripting disabled. | Implemented |
| FR-AUTH-10 | Failed authentication attempts MUST be recorded in the audit trail with an explicit trust marker distinguishing verified from claimed identity. | An audit row of type `login_failed` exists for each failure, carrying a `trusted` flag. | Deferred (RFC: failed-login audit recording) |

**SameSite rationale.** `SameSite=Strict` is not usable: the OIDC
callback is a top-level cross-site navigation, and `Strict` would drop
the session cookie on return from the IdP. `Lax` is therefore required,
and the resulting CSRF exposure is closed by §5.2 rather than by cookie
attributes alone.

### 5.2 Request integrity — CSRF (`FR-CSRF`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-CSRF-01 | Every state-changing endpoint MUST require a valid CSRF token bound to the caller's session. | All 14 mutating endpoints reject a request with a missing or foreign token. | Implemented |
| FR-CSRF-02 | The CSRF token MUST be at least 256 bits, generated per session, and MUST NOT be derivable from the session identifier. | Token is 32 independent random bytes. | Implemented |
| FR-CSRF-03 | Token comparison MUST be constant-time. | Comparison uses a constant-time primitive, not `==` on strings. | Implemented |
| FR-CSRF-04 | The token MUST be exposed to the page in a form usable without inline script injection of secrets. | Surfaced as `<meta name="csrf-token">`; submitted via the `X-CSRF-Token` header. | Implemented |
| FR-CSRF-05 | `GET /auth/logout` MAY be exempt from CSRF validation; the `POST` form MUST NOT be. | Documented intentional exception; the POST variant enforces the token. | Implemented |
| FR-CSRF-06 | Sessions created before CSRF enforcement MAY be granted a single grace request, after which re-authentication is required. | Legacy session logs a warning, is allowed once, then must re-login. | Implemented |

### 5.3 Authorization (`FR-RBAC`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-RBAC-01 | Exactly two roles MUST exist: `admin` and `member`. No guest access. | Role column is constrained to those two values. | Implemented |
| FR-RBAC-02 | An authenticated subject with no `users` row MUST be refused with 403. | Unknown `sub` cannot reach any authenticated page. | Implemented |
| FR-RBAC-03 | A user with `is_active = false` MUST be refused with 403 even with a valid ID token. | Deactivated user cannot log in. | Implemented |
| FR-RBAC-04 | A member MUST see only targets they own, in every view including lists, detail pages, statistics, and exports. | Dashboard counts, `/targets`, `/stats`, and CSV output are all owner-scoped. | Implemented |
| FR-RBAC-05 | Admin-only controls MUST be absent from member-rendered HTML, not merely hidden by CSS. | Member page source contains no admin action markup. | Implemented |
| FR-RBAC-06 | Every admin-only capability MUST be enforced server-side regardless of UI visibility. | Direct request to an admin endpoint as a member returns 403. | Implemented |
| FR-RBAC-07 | The identity mapping MUST key on the OIDC `sub` claim, not on email. | Changing a user's email at the IdP does not create a second account. | **Not met** — the `users` table has no `sub` column; resolution is by email alone (§11 G-16) |

### 5.4 Target management (`FR-TGT`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-TGT-01 | A target MUST support the probe types `http`, `https`, `tcp`, `smtp`, and `tls`. | All five accepted and dispatched to distinct probe logic. | Implemented |
| FR-TGT-02 | A target MUST carry connection details: host, optional port, and optional path. | Persisted and used by the probe. | Implemented |
| FR-TGT-03 | A target MUST carry decision criteria appropriate to its type: expected status code, expected body substring, TLS remaining-days threshold, timeout, retry count, and interval. | Each field is honoured by the matching probe type. | Implemented |
| FR-TGT-04 | A target MUST carry operational attributes: disabled flag, owner, and tags. | Disabled targets are skipped by the scheduler and shown as secondary in the UI. | Implemented |
| FR-TGT-05 | A target MUST record its next scheduled check time. | `next_check_at` is maintained and drives selection. | Implemented |
| FR-TGT-06 | Target creation, update, and deletion MUST be restricted to admins. | Member requests are refused. | Implemented |
| FR-TGT-07 | Deleting a target MUST remove its dependent records without leaving orphans. | States, results, incidents, and attachments are removed. | Implemented |
| FR-TGT-08 | A target MUST record who created and who last updated it. | Both attributes are readable through the API and preserved across export/import. | Not met — see §11 G-05 |
| FR-TGT-09 | Target editing SHOULD be possible through the web UI, not only the API. | An admin can change a target's configuration from `/targets/:id`. | Deferred |
| FR-TGT-10 | Tags MUST match exactly when used for scoping; a tag MUST NOT match by substring. | A window scoped to `api` does not apply to a target tagged `api-v2`. | Not met — see §11 G-09 |

### 5.5 Definition of "normal" (`FR-CHK`)

The original requirement is explicit that "responded" is not the same as
"healthy": each probe type MUST define which responses count as normal.

| ID | Type | Requirement | Status |
|---|---|---|---|
| FR-CHK-01 | HTTP / HTTPS | A check is normal when the connection is established, no timeout occurs, the response status matches `expected_status` (default 200), and — when configured — the body contains `body_contains`. | Implemented |
| FR-CHK-02 | TCP | A check is normal when a TCP connection to the given host and port is established without timing out. | Implemented |
| FR-CHK-03 | TCP (banner) | A configured banner expectation SHOULD be verifiable after connection. | Deferred |
| FR-CHK-04 | SMTP | A check is normal when the port accepts a connection and the server returns a `220` greeting. Ports 25, 465, and 587 are supported. | Implemented |
| FR-CHK-05 | SMTP (extended) | `EHLO`/`HELO` success and STARTTLS availability SHOULD be verifiable as part of the probe. | Deferred — currently exercised only on the email *delivery* path, not the probe path |
| FR-CHK-06 | TLS | A check is normal when the handshake succeeds and remaining certificate validity is at least `tls_threshold_days` (default 30). | Implemented |
| FR-CHK-07 | TLS (chain / revocation) | Chain validation failure and revocation MUST be treated as abnormal. | Partial — handshake failure is abnormal; explicit revocation checking is not performed |
| FR-CHK-08 | TLS (SNI) | An explicit SNI value SHOULD be configurable where the default does not apply. | Deferred |
| FR-CHK-09 | All | A timeout MUST be treated as a failed check. | Implemented |
| FR-CHK-10 | All | Exhausting the retry budget MUST be treated as a failed check. | Implemented |

### 5.6 Monitoring execution (`FR-MON`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-MON-01 | The system MUST use exactly one scheduler trigger, which selects all targets whose next check time has arrived. Multiple Cron registrations MUST NOT be used. | A single `* * * * *` trigger is configured. | Implemented |
| FR-MON-02 | Each tick MUST select only enabled targets whose `next_check_at` has passed. | Disabled and not-yet-due targets are skipped. | Implemented |
| FR-MON-03 | Due targets MUST be probed concurrently within the platform's concurrency limits. | A tick processes the full due set within its budget at the supported scale. | Implemented |
| FR-MON-04 | Retries MUST occur within a single tick and MUST NOT each count as an independent failure. | `retry_count` absorbs a transient blip without advancing the failure counter. | Implemented |
| FR-MON-05 | Every check MUST append a check result record. | One row per check, including during suppression windows. | Implemented |
| FR-MON-06 | State MUST change only when a consecutive-count threshold is reached. | `failure_threshold` consecutive failures produce `down`; `success_threshold` consecutive successes from `down` produce `up`. Defaults are 3. | Implemented |
| FR-MON-07 | Threshold logic MUST be implemented as a pure function, independently testable without a runtime. | `decide_transition` is unit-tested on the host target. | Implemented |
| FR-MON-08 | Notifications MUST be dispatched only on a transition, never on every check. | A target failing continuously produces one notification, not one per minute. | Implemented |
| FR-MON-09 | Flapping below the thresholds MUST NOT produce notifications. | Alternating pass/fail below threshold is silent. | Implemented |

### 5.7 Incident management (`FR-INC`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-INC-01 | A transition to `down` MUST open an incident; the matching recovery MUST resolve it. | Incident lifecycle mirrors state transitions. | Implemented |
| FR-INC-02 | An incident MUST record opening time, resolution time, status, and the cause observed at detection. | All four persisted. | Implemented |
| FR-INC-03 | At most one incident per target MUST be open at any time. | Enforced by the database, not only by application flow. | Not met — see §11 G-11 |
| FR-INC-04 | An admin MUST be able to resolve an open incident manually. | Manual resolution available from `/incidents`. | Implemented |
| FR-INC-05 | Manual resolution MUST capture a structured reason from a closed set, with free text required when "other" is chosen. | Reason codes: `recovered_externally`, `transient`, `target_removed`, `other`. | Implemented |
| FR-INC-06 | The structured reason MUST be machine-readable for later aggregation. | Stored as `[code] free text`, parseable by downstream queries. | Implemented |
| FR-INC-07 | The UI MUST make clear that manual resolution changes neither the check policy nor actual target health. | Explanatory copy present in the resolution dialog. | Implemented |
| FR-INC-08 | Every resolved incident MUST record its duration so that mean-time-to-recovery can be computed. | Duration is present for both automatic and manual resolutions. | Not met — see §11 G-10 |
| FR-INC-09 | Open incidents MUST be presented before resolved ones and MUST be visually distinct. | Open section precedes resolved section. | Implemented |
| FR-INC-10 | The incident state set MUST contain only states the system actually produces. | No unreachable state is accepted by the schema. | Not met — see §11 G-17 |

### 5.8 Notification suppression (`FR-SUP`)

This is the area where terminology and semantics were most frequently
misread, so both are specified explicitly.

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-SUP-01 | A suppression window MUST be definable with a start and end time, both in UTC. | Times stored and displayed with an explicit zone. | Implemented |
| FR-SUP-02 | A window MUST be scopable to a single target, to a tag, or to the whole deployment. | All three scopes accepted. | Implemented |
| FR-SUP-03 | A suppression window MUST NOT specify both a target scope and a tag scope. A window is scoped to a target, to a tag, or to the whole deployment — never to more than one of the three. | The database rejects a window carrying both `target_id` and `target_tag`. | Not met — see §11 G-08 |
| FR-SUP-04 | During a window, checks MUST continue to run. | Check results accumulate throughout. | Implemented |
| FR-SUP-05 | During a window, incidents MUST still be recorded. | Incidents opened during a window appear in `/incidents`. | Implemented |
| FR-SUP-06 | During a window, notifications MUST NOT be dispatched. | No channel receives a message for a suppressed transition. | Implemented |
| FR-SUP-07 | A window whose suppression flag is disabled MUST NOT suppress notifications. | A window with `suppress_notify = false` records but does not suppress. | Not met — see §11 G-07 |
| FR-SUP-08 | An inactive window MUST NOT affect SLA calculation. | Deactivated windows are excluded from SLA queries. | Not met — see §11 G-07 |
| FR-SUP-09 | A window MUST record its creator and last updater. | Both persisted for audit. | Implemented |
| FR-SUP-10 | A window MUST NOT be saveable with an end time at or before its start time. | Rejected at both application and database level. | Partial — validated in the application, not constrained in the schema (§11 G-13) |
| FR-SUP-11 | The UI MUST state the applicable semantics explicitly: checks continue, incidents are recorded, notifications are suppressed *when the window silences*, and SLA excludes the window *when the window excludes*. | Help text present on `/maintenance`, conditioned on the window's two flags. | Not met — help text currently states all four as an unconditional package; see FR-SUP-13 |
| FR-SUP-12 | User-facing text MUST use "notification suppression", never "maintenance". | No user-visible string says "maintenance window". | Implemented |
| FR-SUP-13 | A suppression window MUST state, independently, whether it silences notifications and whether it excludes its time from SLA calculation. The interface MUST present this as named situations (for example "planned maintenance", "known external outage", "expected noise"), not as two unexplained checkboxes. | Creating a window offers a small set of named situations, each stating its own consequence; the underlying record carries two independent flags. | Not met — scheduled for M2 (Phase 3); see [DEC-013](./decision-log.md#dec-013) |

### 5.9 SLA reporting (`FR-SLA`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-SLA-01 | Reporting windows MUST include 24 hours, 7 days, 30 days, and 90 days. | All four selectable. | Implemented |
| FR-SLA-02 | The selected window MUST be reflected in the URL so a view is linkable and survives reload. | `?window=` query parameter drives state. | Implemented |
| FR-SLA-03 | Window selection MUST work without JavaScript. | Selector is link-based, not script-driven. | Implemented |
| FR-SLA-04 | Both gross uptime and SLA uptime MUST be reported, and the difference MUST be explained near the figures. | Both columns present with interpretation text. | Implemented |
| FR-SLA-05 | Suppression-window time MUST be excluded from the SLA denominator. | See formula below. | Not met — see §11 G-12 |
| FR-SLA-06 | Reports MUST be exportable as CSV, honouring the same role scoping as the on-screen view. | CSV for a member contains only owned targets. | Implemented |
| FR-SLA-07 | CSV output MUST conform to RFC 4180 and MUST open correctly in spreadsheet software defaulting to a local codepage. | CRLF line endings, quote escaping, UTF-8 BOM. | Implemented |
| FR-SLA-08 | Per-target CSV export MUST be available in addition to the aggregate export. | Per-row export control present. | Implemented |
| FR-SLA-09 | When a reporting window is entirely excluded from SLA calculation, SLA uptime MUST be reported as not applicable, never as 100%. | The figure renders as an em dash, the same convention already used when `mttr_seconds` has no resolved incident to measure. | Not met — scheduled for M2 (Phase 3); see [DEC-013](./decision-log.md#dec-013) |

**Required SLA formula.** SLA uptime MUST be computed by removing
excluded time — time covered by a window with its SLA-exclusion flag
set (§13 D-2, DEC-013) — from the denominator:

```
effective_window = window_seconds − excluded_seconds
sla_uptime       = (effective_window − downtime_outside_exclusion) / effective_window
                    (not applicable, if effective_window == 0 — FR-SLA-09)
```

This is materially different from subtracting excluded downtime from
the numerator while leaving the denominator at the full window length.
The latter answers "ignore outages during maintenance"; the requirement
is "maintenance time did not happen for SLA purposes". The current
implementation performs the former (§11 G-12).

### 5.10 Notification channels and delivery (`FR-NTF`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-NTF-01 | Three channel types MUST be supported: webhook, Slack, and email. | All three deliverable. | Implemented |
| FR-NTF-02 | Channel endpoints MUST be validated per type at creation and again at update. | Webhook and Slack require `https://`; email requires a single `@`, a dotted domain, and length ≤ 254. | Implemented |
| FR-NTF-03 | A channel MUST be attachable to a target with independent `on_down` and `on_up` selection. | Both flags settable per attachment. | Implemented |
| FR-NTF-04 | A channel MUST be testable on demand, using the same dispatch path as real notifications. | Test send exercises production code, not a mock. | Implemented |
| FR-NTF-05 | Test-send results MUST be presented inline on the page, accessibly announced, and MUST NOT rely on a modal dialog or transient toast alone. | Result rendered in a live region. | Implemented |
| FR-NTF-06 | Rate-limit responses MUST be translated into human-readable retry guidance. | `Retry-After: 90` renders as "Try again in about 1.5 minutes." | Implemented |
| FR-NTF-07 | Email delivery MUST negotiate the strongest authentication the server advertises, preferring SCRAM-SHA-256, then PLAIN, then LOGIN. | Negotiation order verified against a server advertising all three. | Implemented |
| FR-NTF-08 | Email transport MUST use implicit TLS on port 465 and STARTTLS otherwise, with an environment override available. | Both modes exercised. | Implemented |
| FR-NTF-09 | Outgoing mail MUST be RFC 5322 / 2047 / MIME conformant. | Construction delegated to a dedicated builder. | Implemented |
| FR-NTF-10 | The Message-ID domain MUST match the sender domain to avoid relay anti-spoofing rejection. | `<uuid@from-domain>` format. | Implemented |
| FR-NTF-11 | One channel MUST correspond to exactly one recipient; fan-out MUST NOT be performed by blind-copying. | Multiple recipients require multiple channels. | Implemented |
| FR-NTF-12 | Slack channels SHOULD render as native Slack formatting rather than generic JSON. | Block Kit payload with status colour and target link. | **Partial** — a Block Kit document with per-status colour, emoji, mrkdwn section and context block already ships. The open RFC is *enrichment* (header block, structured fields, deep link), not introduction |
| FR-NTF-13 | Delivery attempts and their outcomes SHOULD be persisted, so an operator can answer "was this notified?" after the fact. | A queryable delivery record exists per attempt. | Not met — see §11 G-18 |
| FR-NTF-14 | Delivery failure MUST NOT interrupt monitoring or incident recording. | A failing channel does not prevent state updates. | Implemented |

### 5.11 Audit trail (`FR-AUD`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-AUD-01 | Every state-changing operation MUST be recorded: target, channel, and window create/update/delete; user upsert; manual incident resolution; successful login; configuration import. | One row per operation. | Implemented |
| FR-AUD-02 | Each row MUST record time (UTC, ISO 8601), actor identity, resource type and identifier, action type, previous and new values, outcome, and source IP where applicable. | All fields persisted. | Implemented |
| FR-AUD-03 | The trail MUST be tamper-evident: modification, deletion, or reordering of any row MUST be detectable. | Each row carries a SHA-256 hash over a canonical serialization, chained to its predecessor **under a single total order used identically by the writer and the verifier**. A migration that rewrites the table MUST leave the verification result unchanged. | **Implemented** — Subject 05 (§11 ~~G-30~~). The single total order is the chain's own links, read identically by both `verify_chain` and `current_head_hash` (one walk, not two) |
| FR-AUD-04 | The canonical serialization MUST be version-tagged and field-order-stable. | Format version plus a fixed 11-field delimited layout; pinned by unit tests. | Implemented |
| FR-AUD-05 | An integrity check MUST be available to an admin, classifying every row as verified, legacy, tampered, or **orphaned**. A row that was altered and a row that is unreachable because another row was deleted MUST NOT be reported as the same class. | `GET /api/admin/audit/verify` returns the four-way classification. With one row's content edited, that row alone is `tampered`. With one row deleted, the rows after it are `orphaned` and none is `tampered`. | **Implemented** — Subject 05 (§11 ~~G-30~~). Four-way classification built; T-22/T-23 confirm the right row is named in each case. *(Round 3, 2026-07-31: an independent review found the initial cycle-termination fix double-classified the row where a cycle closes, violating this requirement's own "MUST NOT be reported as the same class" in a new way — a row appeared in `tampered_rows` twice. Fixed: `cycle_at` names the looping row separately, not as a fifth class; T-23e adds a standing partition-invariant guard, run by every test in the module.)* |
| FR-AUD-06 | Audit rows MUST NOT be deleted by retention processing, because deletion breaks the chain. | With a retention policy row for `audit_logs` **present** in the database, a retention pass deletes no audit row. *(The former criterion — "no scheduled job removes audit rows" — tested the absence of configuration rather than the presence of a guard.)* | **Implemented** — Subject 04 (§11 ~~G-04~~). **Confirmed against the local D1 runtime, subject 07a** — the `audit_logs` policy row migration 0003 removes was reinserted by hand (the exact scenario this requirement names), a full retention pass ran against real local D1 with three eligible audit rows present, and zero were deleted (`.git-exclude/evidence/subject-07a-step2-dr-lif-06-07-fr-aud-06.log`). |
| FR-AUD-07 | System-initiated events MUST be recordable without a corresponding user account. | An event by actor `system` inserts successfully. | **Implemented** — Subject 06 (§11 ~~G-03~~). `actor_id` is a snapshot, not a foreign key; T-24/T-29 confirm the `system` actor inserts |
| FR-AUD-08 | Failure to write an audit row MUST be surfaced, not silently discarded. | A failed audit insert is logged at error level and does not pass unnoticed. | **Implemented** — Subject 07 (§11 ~~G-26~~). `db::audit::log_or_report`/`log_system_or_report` log at error level on failure; T-33 confirms resource type, id, action type and actor are named |
| FR-AUD-09 | Audit history SHOULD be mirrored outside the primary database, so that wholesale table loss is detectable and recoverable. | An off-system append-only copy exists. | Deferred (RFC: audit-log mirror) |
| FR-AUD-10 | The audit view MUST allow inspecting before/after values without leaving the page or relying on hover. | Values expand in place via a disclosure control. | Implemented |
| FR-AUD-11 | The outcome of an audit write MUST be observable to the operator who initiated the operation, not only in platform logs. | When the audit write for a mutation fails, the operator sees an explicit indication in the operation's result panel; the failure is additionally recorded at error level with resource type, identifier, action and actor, and never with the changed values. | **Implemented** — Subject 07 (§11 ~~G-26~~). `X-Audit-Warning` propagates Core → Gateway → browser; T-32 confirms the warning renders alongside, not instead of, the success message |

**Chain scope note.** The hash chain proves that the retained rows have
not been altered or reordered. It cannot detect deletion of the entire
table and its replacement with a self-consistent forgery; that gap is
what FR-AUD-09 addresses.

### 5.12 User administration (`FR-USR`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-USR-01 | An admin MUST be able to create and update users, setting role and active flag. | Upsert form available in `/settings`. | Implemented |
| FR-USR-02 | User deletion MUST NOT be offered; deactivation MUST be used instead. | No delete control or endpoint exists. | Implemented |
| FR-USR-03 | The reason for deactivation-instead-of-deletion MUST be explained in the UI. | Help text states that the audit trail references the account. | Implemented |
| FR-USR-04 | A deactivated user MUST remain visible in historical audit records. | Past rows continue to resolve the actor. | Implemented |
| FR-USR-05 | Email addresses MUST be treated case-insensitively for identity purposes. | Two casings of one address cannot become two accounts. | Not met — see §11 G-16 |
| FR-USR-06 | Changing the role of the currently signed-in admin SHOULD require explicit confirmation. | Confirmation step present. | Deferred |

### 5.13 Configuration migration (`FR-MIG`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-MIG-01 | Configuration MUST be exportable as a self-describing, versioned document. | Export carries a schema version and timestamp. | Implemented |
| FR-MIG-02 | Secrets MUST NEVER appear in an export. | No credential material in output. | Implemented |
| FR-MIG-03 | Audit history MUST NOT be included in a configuration export. | Export omits audit rows. | Implemented |
| FR-MIG-04 | User records MUST be includable or excludable at the operator's choice. | `include_users` flag honoured. | Implemented |
| FR-MIG-05 | Import MUST default to a dry run; applying changes MUST be an explicit action. | Apply flag defaults to off. | Implemented |
| FR-MIG-06 | Import MUST report all validation errors in one pass rather than stopping at the first. | Multiple independent errors surface together. | Implemented |
| FR-MIG-07 | Conflict handling MUST be selectable: skip, replace, or fail. | All three policies available. | Implemented |
| FR-MIG-08 | An import MUST produce a system state equivalent to having created the same objects through the normal path. | Imported targets are immediately monitorable. | Not met — see §11 G-05, G-06 |
| FR-MIG-09 | Import MUST be recorded in the audit trail. | One audit row per applied import. | Implemented |
| FR-MIG-10 | Import MUST resolve every cross-reference carried by the document against the target deployment **before** any write, and MUST report all unresolvable references together. | A document referencing users absent from the deployment is rejected with a count of every affected object, in dry run, having written nothing. Unresolvable references MUST NOT be silently remapped. | Not met — new requirement, 2026-07-28 (§11 G-31) |
| FR-MIG-11 | Applying an import MUST NOT delete monitoring history, incidents, or channel attachments belonging to objects it updates. | A `replace` import onto an existing target leaves its check results, incidents and attachments intact. | Not met — new requirement, 2026-07-28 (§11 G-22) |

**Equivalence requirement (FR-MIG-08).** "Equivalent" is a strong claim
and is the correct bar: an imported target that lacks a state row or a
mandatory provenance column is a latent failure that surfaces later, at
monitoring time, far from the import that caused it.

### 5.14 Web interface (`FR-UI`)

#### 5.14.1 Structure and navigation

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-UI-01 | Navigation MUST be grouped by operator intent, not presented as a flat link list. | Three groups: Observe, Operate, Verify. | Implemented |
| FR-UI-02 | The admin-only group MUST be entirely absent for members, not merely disabled. | Member markup contains no Verify group. | Implemented |
| FR-UI-03 | Personal account controls MUST be separated from workspace navigation. | Account and logout live in the user chip. | Implemented |
| FR-UI-04 | Each page MUST have exactly one `<main>` landmark and one visible `<h1>`. | Verified per route. | Implemented |
| FR-UI-05 | Each screen MUST have one primary responsibility and one primary action. | No screen presents competing primary actions. | Implemented |
| FR-UI-06 | Multi-section pages MUST express section state in the URL so views are linkable and reload-safe. | Target detail uses `?tab=`; statistics use `?window=`. | Implemented |

#### 5.14.2 Screen requirements

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-UI-07 | The dashboard MUST answer "is anything wrong, and what needs action now" above all other content. | Metric strip, then open incidents, then supporting detail. | Implemented |
| FR-UI-08 | Dashboard sections carrying no information MUST be omitted rather than rendered empty. | Status breakdown is hidden when every count is zero. | Implemented |
| FR-UI-09 | Empty states MUST be actionable or reassuring, never bare. | "All clear — no open incidents right now." | Implemented |
| FR-UI-10 | Target detail MUST consolidate health, results, notification routing, and settings for one target. | Four sections on one route. | Implemented |
| FR-UI-11 | A detail view MUST fetch only what the active section needs. | Inactive-section queries are not issued. | Implemented |
| FR-UI-12 | Destructive actions MUST be separated from routine controls and MUST require confirmation. | Delete presented in a distinct region with confirmation. | Implemented |
| FR-UI-13 | Lists MUST support the filters an operator needs to locate a target: status, type, owner (admin), and tag. | Filter bar present with associated labels. | Partial — filtering is available on primary lists; coverage is not uniform across every list |
| FR-UI-14 | Times MUST be rendered in a machine-readable element carrying the source instant, so local rendering is possible without ambiguity. | `<time datetime="…">` used throughout. | Implemented |

#### 5.14.3 Interaction constraints

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-UI-15 | Browser dialog primitives (`alert`, `prompt`) MUST NOT be used. | Zero occurrences in the source tree. | Implemented |
| FR-UI-16 | Operation results MUST be rendered inline and announced to assistive technology. | Live-region result panels. | Implemented |
| FR-UI-17 | Controls MUST NOT be hover-only. | All actions reachable by keyboard and touch. | Implemented |
| FR-UI-18 | Server-rendered links MUST be preferred over script-driven interactions. | Navigation works with scripting disabled. | Implemented |
| FR-UI-19 | Loading states MUST preserve layout to avoid disorientation. | No layout shift on state change. | Implemented |
| FR-UI-20 | Form errors MUST appear both beside the offending field and in a form-level summary. | Both present. | Implemented |

### 5.15 Operational endpoints (`FR-OPS`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| FR-OPS-01 | An unauthenticated health endpoint MUST be available for external monitoring. | `GET /healthz` returns 200 without credentials. | Implemented |
| FR-OPS-02 | The health endpoint MUST NOT require the private Worker to be reachable. | Gateway answers independently. | Implemented |
| FR-OPS-03 | The health endpoint MUST NOT disclose configuration or internal state. | Response body is a fixed status document. | Implemented |
| FR-OPS-04 | Login MUST be rate-limited per source address. | 10 per minute and 50 per hour. | Implemented |
| FR-OPS-05 | Channel test sending MUST be rate-limited per source address. | 15 per minute. | Implemented |
| FR-OPS-06 | Rate-limit responses MUST include retry timing. | `Retry-After` present. | Implemented |
| FR-OPS-07 | Automated-client defence SHOULD be available on the public login form, and MUST remain confined to public forms. | Challenge widget on `/auth/login` only. | Deferred (RFC: Turnstile activation) |

---

## 6. Data requirements

### 6.1 Storage allocation (`DR-STO`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| DR-STO-01 | Relational business data MUST be held in D1 as the system of record. | Targets, states, results, incidents, windows, channels, attachments, audit rows, users, retention policies. | Implemented |
| DR-STO-02 | KV MUST hold only session and short-lived data; it MUST NOT hold the authoritative copy of business data. | Sessions, OIDC transient state, JWKS cache, rate-limit buckets. | Implemented |
| DR-STO-03 | R2 MUST be used for large unstructured artifacts: archives, snapshots, exports. | Archived results and export documents. | Implemented |
| DR-STO-04 | Loss of KV contents MUST NOT cause loss of business data. | Operators are logged out; nothing else is affected. | Implemented |

### 6.2 Entity model (`DR-ENT`)

The required relational structure:

```
users
  ├─ targets.owner_id                 1:N
  ├─ notification_channels.owner_id   1:N
  └─ audit_logs.actor_id              1:N   (snapshot reference — see DR-INT-04)

targets
  ├─ target_states.target_id          1:1
  ├─ check_results.target_id          1:N
  ├─ incidents.target_id              1:N   (at most one open — FR-INC-03)
  ├─ maintenance_windows.target_id    0:N   (optional scope)
  └─ target_notifications.target_id   N:M ─ notification_channels

retention_policies                    keyed by table name
```

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| DR-ENT-01 | Every target MUST have exactly one state row, created with the target. | No target exists without a state row, by any creation path. | Partial — holds on the normal path, not on import (§11 G-06) |
| DR-ENT-02 | Deleting a target MUST cascade to its dependent rows. | Foreign keys declare cascade behaviour. | Implemented |
| DR-ENT-03 | Tag scoping MUST be expressible as an exact-match relation. | A normalized tag relation, or exact-match JSON evaluation. | Not met — see §11 G-09 |
| DR-ENT-04 | Consecutive-count thresholds MUST be treated as target configuration and MUST survive export/import. | Thresholds reproduced after a round trip. | Not met — see §11 G-06 |

### 6.3 Integrity constraints (`DR-INT`)

The guiding rule: **application-level validation is necessary but not
sufficient.** Data reaches these tables through the API, the CLI,
configuration import, and direct database access during operations. A
constraint that exists only in Rust does not hold for the other three.

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| DR-INT-01 | Boolean columns MUST be constrained to 0 or 1. | `CHECK (col IN (0,1))` present. | Not met — §11 G-13 |
| DR-INT-02 | Numeric columns MUST be range-constrained: port 1–65535; expected status 100–599; timeout 1–300 s; retries 0–10; interval 1–1440 min; TLS threshold ≥ 0. | Corresponding `CHECK` constraints present. | Not met — §11 G-13 |
| DR-INT-03 | Interval columns MUST enforce start < end. | `CHECK (start_at < end_at)`. | Not met — §11 G-13 |
| DR-INT-04 | The audit actor reference MUST tolerate non-user actors and MUST NOT be invalidated by later user-row changes. | A `system` actor row inserts successfully; the actor is stored as a snapshot rather than a live foreign key. | **Implemented** — Subject 06 (§11 ~~G-03~~). `sql/0004` drops the foreign key in favour of `CHECK (actor_id != '')`; T-28 confirms deactivating/renaming a user alters no historical row |
| DR-INT-05 | At most one open incident per target MUST be enforced by the database. | Partial unique index on open incidents. | Not met — §11 G-11 |
| DR-INT-06 | A suppression window MUST NOT specify both a target scope and a tag scope. | Constraint or explicit precedence logic. | Not met — §11 G-08 |
| DR-INT-07 | Timestamps MUST use one format across schema defaults and application writes. | RFC 3339 `YYYY-MM-DDTHH:MM:SSZ` everywhere. | Not met — §11 G-14 |
| DR-INT-08 | Indexes MUST exist for every access path used by a list or join in normal operation. | Owner lookups, channel-to-target joins, incident ordering, window overlap, audit filtering. | Partial — §11 G-15 |
| DR-INT-09 | A migration that rewrites a table participating in the audit hash chain MUST preserve every column value exactly, including nulls. | Chain verification returns an identical classification immediately before and immediately after the migration: the **whole** `ChainVerification` compared, not a subset — all four counts (verified, legacy, tampered, orphaned), the same identifiers in both `tampered_rows` and `orphaned_rows`, and the same `cycle_at`. *(Amended 2026-08-01: the original criterion named "verified, legacy and tampered counts", which was the complete set when it was written on 2026-07-28 and is no longer. A migration that orphaned rows, or introduced a cycle, would have satisfied it. Comparing a subset of a report that has since grown is how a critical guard goes quietly weak — the criterion now names the whole object rather than enumerating today's fields.)* | **Implemented** — Subject 06 (§11 ~~G-03~~). T-25 compares the whole `ChainVerification` immediately before/after `sql/0004`; T-29c confirms every pre-existing `row_hash` is preserved byte-for-byte |

**Timestamp rationale (DR-INT-07).** Scheduling and window-overlap
logic compare timestamps **as strings**. Mixing SQLite's
`YYYY-MM-DD HH:MM:SS` default with the application's RFC 3339 form
produces comparisons that are silently wrong rather than erroneous,
because `' '` and `'T'` differ in ordinal value. This is a correctness
requirement, not a cosmetic one.

### 6.4 Lifecycle and retention (`DR-LIF`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| DR-LIF-01 | Retention periods MUST be configurable per data class. | A retention policy table drives cleanup. | Implemented |
| DR-LIF-02 | Check results MUST be subject to retention, with archival before deletion. | Archived to R2, then removed. | Implemented |
| DR-LIF-03 | Incidents MUST be subject to retention, with archival before deletion. | As above. | Implemented |
| DR-LIF-04 | Audit rows MUST be exempt from retention deletion. | No policy or code path deletes them. | **Implemented** — Subject 04 (§11 ~~G-04~~) |
| DR-LIF-05 | Retention processing MUST run on a schedule without operator intervention. | Executed by the scheduled job. | Implemented |
| DR-LIF-06 | A retention pass MUST NOT delete a record that has not been successfully archived in that same pass. The set of records deleted MUST be identical to the set successfully written to the archive. **This holds regardless of a policy's `archive_to_r2` flag: for a class where archival-before-deletion is otherwise required (DR-LIF-02, DR-LIF-03), `archive_to_r2 = 0` is a configuration error, not a valid way to skip archiving.** | With more eligible records than one archive batch holds, the count archived equals the count deleted, and every deleted record is present in an archive object. A `check_results` or `incidents` policy with `archive_to_r2 = 0` deletes nothing and reports the misconfiguration. | Implemented — see §11 G-20 resolution. **Confirmed against the local D1 runtime, subject 07a**: 150 eligible `check_results` rows (more than one `RETENTION_BATCH_SIZE`) produced exactly two archive batches (100 + 50), and the union of their ids matched the 150 seeded ids exactly — the archived set and the deleted set are identical, by id comparison, not just count (`.git-exclude/evidence/subject-07a-step2-dr-lif-06-07-fr-aud-06.log`). |
| DR-LIF-07 | A failed archive write MUST abort the retention pass for that data class, leaving every not-yet-archived record in place. A subsequent pass MUST resume **without loss**. It MAY re-archive a batch that was archived but not yet deleted — *duplication is accepted in exchange for never losing a record*, see **DEC-022**. | With the archive write forced to fail, no record is deleted; a later successful pass archives and deletes each eligible record exactly once. | **Implemented — confirmed against the local D1 runtime, subject 07a, both halves.** Half 1 (abort): with the `LOG_BUCKET` R2 binding absent, a pass over 150 eligible `check_results` rows failed at `env.bucket("LOG_BUCKET")` and deleted zero rows. Half 2 (resume): with the binding restored, a subsequent pass against the same unmodified data archived and deleted all 150 rows across two batches with zero id overlap between them — every eligible record archived and deleted exactly once, not merely "no error." (`.git-exclude/evidence/subject-07a-step2-dr-lif-06-07-fr-aud-06.log`) |

### 6.5 Schema migration (`DR-MIG`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| DR-MIG-01 | Applying all migrations in order to an empty database MUST succeed. | A fresh deployment provisions without error. | Implemented — `scripts/check-migrations.sh`, CI job `migrations` |
| DR-MIG-02 | Migrations MUST be ordered and numbered, and **the applied effect of a released migration MUST NOT change**. "Released" means shipped under a version tag. Comments and formatting MAY be corrected; no statement affecting schema or data may be added, removed or altered. | A database built from the pre-edit file and one built from the post-edit file are **structurally identical**: same tables, and for each, identical `PRAGMA table_info`, `index_list` and `foreign_key_list` output, the same index set, and the same seeded rows. *(Compare structure, not `.schema` text — that reproduces inline comments, so a comment-only edit would fail a naive reading. Criterion corrected 2026-07-28 after it misreported on first use.)* | **Not met** — `sql/0001_initial.sql` shipped at tag 0.1.0 and had DDL added at 0.27.2, which is the direct cause of G-01. *(Reworded 2026-07-28: the previous wording forbade correcting a comment, which serves nothing — two databases built either side of a comment-only edit are byte-identical in schema. The rule is about applied effect, not bytes.)* |
| DR-MIG-03 | A migration MUST NOT assume conditional column addition, which the platform does not support. | No reliance on "add column if not exists". | Implemented |
| DR-MIG-04 | The database MUST be recoverable from an export plus retained archives. | Documented restore procedure. | Implemented |
| DR-MIG-05 | The migration set MUST be verified to apply cleanly to an empty database on every change, mechanically. | A build gate applies every file in `sql/` in filename order to a fresh database and fails the build on any error. | Implemented — `scripts/check-migrations.sh`, wired into `.github/workflows/ci.yml` as job `migrations` |

---

## 7. Non-functional requirements

### 7.1 Accessibility (`NFR-A11Y`)

Accessibility requirements are normative and derive from P-2. They apply
to every page without exception.

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-A11Y-01 | All pages MUST meet WCAG 2.1 AA contrast. | Contrast is asserted by automated test, not by inspection. | Implemented |
| NFR-A11Y-02 | Contrast MUST be verified before deployment, and a regression MUST break the build. | 25 colour pairs pinned across light and dark themes in a unit test. | Implemented |
| NFR-A11Y-03 | Status MUST NEVER be conveyed by colour alone. | Every status badge carries a shape marker and a text label. | Implemented |
| NFR-A11Y-04 | Every page MUST expose semantic landmarks. | Banner, navigation, main, and contentinfo roles present. | Implemented |
| NFR-A11Y-05 | The first focusable element on every page MUST be a skip link to the main content. | Verified per route. | Implemented |
| NFR-A11Y-06 | Focus MUST be visibly indicated on all interactive elements. | A consistent focus ring is applied. | Implemented |
| NFR-A11Y-07 | All functionality MUST be operable by keyboard, using standard element behaviour rather than scripted focus management. | Native interactive elements throughout. | Implemented |
| NFR-A11Y-08 | Motion MUST be suppressed when the user has requested reduced motion. | Transitions disabled under the corresponding media query. | Implemented |
| NFR-A11Y-09 | Pages MUST remain readable and operable with CSS unavailable. | Semantic markup; tables not used for layout. | Implemented |
| NFR-A11Y-10 | Core functionality MUST remain available with JavaScript disabled. | Viewing and form submission work without scripting. | Implemented |
| NFR-A11Y-11 | The current navigation location MUST be programmatically indicated. | `aria-current` on the active item. | Implemented |
| NFR-A11Y-12 | Form controls MUST have programmatically associated labels. | Every input carries a label association. | Implemented |
| NFR-A11Y-13 | On small viewports, primary form actions MUST remain reachable without scrolling past long forms. | Action bar pinned at the bottom on narrow screens. | Implemented |

### 7.2 Security (`NFR-SEC`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-SEC-01 | The public attack surface MUST be minimized: only the interface Worker may be internet-reachable. | The private Worker has no public route and no development subdomain. | Implemented |
| NFR-SEC-02 | Inter-Worker calls MUST be authenticated with a shared secret and MUST fail closed. | A request without the token is rejected. | Implemented |
| NFR-SEC-03 | Caller identity MUST be propagated explicitly rather than re-derived by the private Worker. | Identity headers accompany each internal call. | Implemented |
| NFR-SEC-04 | Security headers MUST be applied to all HTML responses: CSP, `nosniff`, frame denial, referrer policy, and a restrictive permissions policy. | Present on every HTML response. | Implemented |
| NFR-SEC-05 | HSTS MUST be applied in production. | Sent with a six-month max-age including subdomains. | Implemented |
| NFR-SEC-06 | Post-login redirects MUST be restricted to same-origin absolute paths. | Scheme-relative, protocol-relative, and cross-host targets fall back to the root. | Implemented |
| NFR-SEC-07 | The environment MUST default to the most restrictive setting when unset. | Unset environment behaves as production. | Implemented |
| NFR-SEC-08 | Development fallback values MUST NOT be deployable to production. | Startup check fails hard if a known development value is present. | Implemented |
| NFR-SEC-09 | Secrets MUST NOT appear in the repository, the release archive, or any export. | Verified by packaging exclusions and export content. | Implemented — neither `wrangler.toml` is tracked; the `.example` templates carry no secret values (§11 G-21) |
| NFR-SEC-10 | Dependencies MUST be scanned against a vulnerability advisory database on every change and on a recurring schedule. | Scan runs in CI on push and weekly, **using an invocation the installed `cargo-audit` accepts**. | Implemented — confirmed in a real GitHub Actions run (PR #2, run `30455673409`): the job fetched 1173 advisories and scanned 224 crates, not the exit-2 failure the prior `--locked` form produced. §11 G-32. |
| NFR-SEC-11 | A suppressed advisory MUST carry a written rationale and explicit criteria for re-evaluation. | Suppression file documents scope, threat-model reasoning, and revisit trigger. | Implemented |
| NFR-SEC-12 | Security-relevant primitives MUST come from established, audited libraries rather than bespoke implementations. | No hand-rolled cryptography. | Implemented |
| NFR-SEC-13 | Application code MUST NOT use `unsafe`. | No `unsafe` blocks outside dependencies. | Implemented |
| NFR-SEC-14 | The repository MUST NOT contain a file that a deployment tool will consume as configuration without an explicit operator copy step. Configuration MUST be supplied as a template. | No `wrangler.toml` is tracked; `wrangler.toml.example` is tracked; `wrangler.toml` is ignored by version control. | Implemented |
| NFR-SEC-15 | Any credential value that has appeared in the repository or its history MUST be rejected at request time in **every** environment, unconditionally. The rejection MUST NOT be conditioned on a variable that the shipped configuration sets. | A request presenting a known published fallback value is refused when the declared environment is `development`, when it is `production`, and when it is unset. | Implemented — `find_leaked_fallback` takes no environment parameter; T-11 in `crates/gateway/src/env_check/tests.rs` and `crates/core/src/env_check/tests.rs` |

### 7.3 Scale and performance (`NFR-PERF`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-PERF-01 | The system MUST complete a full due-target sweep within one scheduling interval at the supported scale. | Several hundred targets processed within one minute. | Implemented |
| NFR-PERF-02 | The supported scale MUST be stated explicitly, together with the mechanism that limits it. | Documented as a few hundred targets; single-writer audit chain is the limiting factor. | Implemented |
| NFR-PERF-03 | Exceeding the supported scale MUST degrade visibly rather than silently. | Overrun is observable in logs. | Partial — overrun is visible in platform logs but is not surfaced in the product |
| NFR-PERF-04 | Data access paths used in normal operation MUST be index-supported. | See DR-INT-08. | Partial — §11 G-15 |

### 7.4 Reliability (`NFR-REL`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-REL-01 | A failure in notification delivery MUST NOT prevent monitoring, state updates, or incident recording. | Verified by fault injection on the delivery path. | Implemented |
| NFR-REL-02 | A single failing target MUST NOT prevent other targets in the same sweep from being checked. | Per-target isolation. | Implemented |
| NFR-REL-03 | Configuration and history MUST be recoverable after loss of the primary database. | Export plus archive restore procedure documented. | **Partial** — the restore path depends on retention archives (§11 G-20) and configuration import (§11 G-05, G-06, G-22), all currently defective |
| NFR-REL-04 | Recovery procedures MUST be written down, not reconstructed at incident time. | Documented in the operations chapter. | Implemented |
| NFR-REL-05 | Concurrent writers MUST NOT be able to fork the audit chain. | Enforced by the single-writer property; any change to that property requires serialization. | Implemented (by constraint) |

### 7.5 Quality gates and testability (`NFR-QA`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-QA-01 | Business logic MUST be testable without a platform runtime. | Pure functions unit-tested on the host target. | Implemented |
| NFR-QA-02 | Interface rendering MUST be testable without a browser. | Rendering functions return strings and are asserted directly. | Implemented |
| NFR-QA-03 | Runtime-dependent tests MUST be limited to what genuinely requires the runtime. | Only cryptographic behaviour is runtime-tested. | Implemented |
| NFR-QA-04 | The build MUST be free of warnings; warnings MUST be treated as errors. | Warnings are denied by the enforcing CI job. | **Implemented** — `.github/workflows/ci.yml:42` fixed and confirmed in a real Actions run (2026-07-29, Subject 03c, run `30460161440`); gate-fails-on-violation also confirmed (run `30460920132`). §11 ~~G-33~~ |
| NFR-QA-05 | Formatting MUST be mechanically enforced. | Format check gates the build. | **Implemented** — same fix, same confirmation. §11 ~~G-33~~ |
| NFR-QA-06 | Dependency resolution MUST be reproducible; lockfile drift MUST fail the build. | All gates run in locked mode. | **Implemented** — same fix, same confirmation. §11 ~~G-33~~ |
| NFR-QA-07 | Both deployable Workers MUST be verified to compile for the target platform. | Platform-target build check for each. | Implemented |
| NFR-QA-08 | Test design MUST derive from the specification, not from the implementation. | Tests assert specified behaviour, not incidental behaviour. | Implemented |
| NFR-QA-09 | Requirements marked `Not met` in this document SHOULD each acquire a regression test as they are closed. | A test exists that would fail against the pre-fix behaviour. | Not met — no such tests exist yet. **Binding as a merge condition from v0.28.0** |
| NFR-QA-10 | Every requirement whose failure mode is "the system cannot be provisioned" MUST have a build gate, not only a test. | DR-MIG-01 is enforced by DR-MIG-05's gate. | Implemented — `scripts/check-migrations.sh`, CI job `migrations` |

### 7.6 Internationalization (`NFR-I18N`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-I18N-01 | The interface MUST support multiple display languages. | An operator can use the product in a language other than English. | **Not met** |
| NFR-I18N-02 | User-facing strings MUST be externalized from rendering logic. | No user-visible literal is embedded in a rendering function. | Not met |
| NFR-I18N-03 | Language selection MUST be explicit and MUST persist for the operator. | Selection survives reload. | Not met |
| NFR-I18N-04 | Locale handling MUST NOT compromise the accessibility guarantees in §7.1. | Contrast pinning and semantics hold in every locale. | Not applicable until NFR-I18N-01 is met |
| NFR-I18N-05 | Timestamps MUST remain unambiguous across locales. | Machine-readable instants with explicit zone. | Implemented |

**This is a specified requirement with no implementation and no
tracking artifact.** The project's development instructions state that
the GUI must support multiple languages. The shipped interface is
English-only, and no RFC covers the work. Either an RFC should be
opened or the requirement should be formally withdrawn; leaving it
stated-but-unowned is the failure mode the RFC lifecycle policy calls
silent withdrawal. See §13 D-3.

### 7.7 Maintainability (`NFR-MNT`)

| ID | Requirement | Acceptance criteria | Status |
|---|---|---|---|
| NFR-MNT-01 | Module boundaries MUST reflect functional boundaries. | Crate split matches responsibility split. | Implemented |
| NFR-MNT-02 | Cross-component data contracts MUST be defined in one place. | A single shared crate holds the shared types. | Implemented |
| NFR-MNT-03 | Presentation tokens MUST be centralized; raw colour values MUST NOT appear in component styles. | Component styles reference token names only. | Implemented |
| NFR-MNT-04 | Design decisions MUST be recorded with their rationale and their re-evaluation criteria. | Decisions carry "why" and "revisit when". | Implemented |
| NFR-MNT-05 | Deferred work MUST be recorded with the reason for deferral. | Roadmap entries state why, not only what. | Implemented |

---

## 8. Environment and compatibility constraints

| ID | Constraint | Status |
|---|---|---|
| CON-01 | Implementation language is Rust, 2024 edition. | Implemented |
| CON-02 | Module layout follows the 2018-and-later style; `mod.rs` is not used. | Implemented |
| CON-03 | The deployment platform is Cloudflare Workers, with D1, KV, R2, and Cron Triggers. | Implemented |
| CON-04 | The deployment tool is Wrangler v4. | Implemented |
| CON-05 | Exactly one scheduled trigger is registered. | Implemented |
| CON-06 | Identity is supplied by an external OIDC provider. | Implemented |
| CON-07 | Dependency licences must be compatible with Apache-2.0. | Implemented |
| CON-08 | The system is deployed as a single tenant per deployment. Multi-tenancy within one deployment is out of scope. | **Implemented** — settled by DEC-008 (2026-07-28); §13 D-1 closed |
| CON-09 | Documentation and code comments are written in English. | **Partial** — `package.sh` header and one `ROADMAP.md` phrase are Japanese (§11 G-24) |
| CON-10 | The project is licensed Apache-2.0; the author is credited without a contact address. | Implemented |

---

## 9. Traceability

Mapping from the original requirement sources to this specification.
"Changed" marks a deliberate supersession, with the reason.

| Original requirement (v1 drafts) | Now specified as | Change |
|---|---|---|
| Cloudflare Access as mandatory entry authentication | FR-AUTH-01..04 | **Changed** — replaced by generic OIDC to avoid identity-layer lock-in; Access remains optional in front |
| Leptos 0.8 SSR frontend | FR-UI-*, NFR-QA-01..02 | **Changed** — replaced by server-side pure functions for host-target testability and dependency reduction |
| Two-role RBAC, no guest | FR-RBAC-01..03 | Unchanged |
| Turnstile limited to public forms | FR-OPS-07 | Unchanged in intent; not yet activated |
| Probe types HTTP/HTTPS/TCP/SMTP + extensions | FR-TGT-01, FR-CHK-01..08 | **Extended** — TLS promoted to a first-class type |
| Per-protocol "normal" definitions | FR-CHK-01..10 | Unchanged |
| Single scheduler processing due targets | FR-MON-01..03 | Unchanged |
| Consecutive-count state transitions | FR-MON-06 | Unchanged |
| Suppress notifications during maintenance; no duplicate alerts | FR-SUP-*, FR-MON-08 | **Clarified** — renamed to notification suppression with four explicit semantics |
| D1 authoritative, KV auxiliary, R2 bulk | DR-STO-01..03 | Unchanged |
| Retention with deletion or archival | DR-LIF-01..05 | **Clarified** — audit rows explicitly exempted |
| Audit trail of principal operations | FR-AUD-01..02 | **Strengthened** — hash chaining and verification added |
| Backup and disaster recovery | FR-MIG-*, NFR-REL-03..04 | Unchanged |
| Notification channels "extensible in future" | FR-NTF-01..11 | **Concretized** — three types specified |
| Multi-tenant management | CON-08, §13 D-1 | **Unresolved** — see open decision |
| Multilingual GUI | NFR-I18N-01..03 | **Unimplemented** — see §13 D-3 |

---

## 10. Development-process requirements

These come from the project's development instructions and are binding
on contributors, not on the running system.

| ID | Requirement | Status |
|---|---|---|
| PRQ-01 | Design precedes implementation: requirements → external design → internal design → program design → implementation → test. | Implemented |
| PRQ-02 | Proposals and their rationale are recorded as RFCs; the folder determines lifecycle state and the status field mirrors it. | Implemented |
| PRQ-03 | Completed RFCs are retained, never deleted; RFC numbers are never reused or renumbered. | Implemented |
| PRQ-04 | Source files are split along logical boundaries; splitting is considered above 300 effective lines and strongly indicated above 500. | Partial — the two Worker crates contain files well above the threshold |
| PRQ-05 | Tests live in sibling modules, not inline in implementation files. | **Not met** — 40 implementation files carry an inline `#[cfg(test)] mod tests`; zero sibling `tests.rs` files exist (§11 G-23). New tests MUST comply from v0.28.0 |
| PRQ-06 | Formatting is run once after implementation completes, and the formatted output is not re-reviewed line by line. | Implemented |
| PRQ-07 | Releases are cut at logical boundaries — a resolved RFC, a completed theme, a finished audit — not on every session. | Implemented |
| PRQ-08 | Release archives carry the version in the filename and unpack flat, with no intermediate parent directory. | Implemented — `package.sh`'s `--transform` removed (Subject 03a); verified by extraction, not by reading the script *(Note: GitHub's automatic source archives carry a `<repo>-<tag>/` prefix and therefore cannot satisfy this requirement; a custom artifact is required, not merely preferred — see §11 ~~G-34~~.)* |
| PRQ-09 | Version is single-sourced from the workspace manifest. | Implemented |
| PRQ-10 | Change history is tracked in the changelog and roadmap. | Implemented |
| PRQ-11 | The README stays concise and follows the fixed six-section structure; full documentation lives in the documentation tree and remains mdBook-compatible. | Implemented — mdBook compatibility was **broken** until 2026-07-28 (`book.toml` carried `multilingual`, removed from the schema in mdBook 0.5; `mdbook build` exited 101, so the README's `mdbook serve docs` instruction did not work). Fixed; the tree now renders |
| PRQ-12 | Documentation is organized by reader persona: newcomer, intermediate user, maintainer. | Implemented |
| PRQ-13 | Licence text is not reproduced in the README; the licence files and badges carry it. | Implemented |
| PRQ-15 | The published release notes MUST be the curated `CHANGELOG.md` section for the version being released, not a generated summary of commits. A release whose changelog section is missing or empty MUST fail rather than publish. *Acceptance: publishing a scratch tag whose changelog section carries a distinctive operator instruction yields a release body containing that instruction verbatim; publishing a scratch tag with no dated changelog section fails the workflow and creates no release.* | **Implemented** — Subject 04a (§11 ~~G-35~~) |
| PRQ-14 | A release archive MUST contain exactly the tracked content of the tagged commit, MUST be reproducible by any party holding that tag, and MUST be produced by an automated, observable process rather than a local invocation. | **Implemented** — Subject 03d (§11 ~~G-34~~). `package.sh` builds via `git archive` over the version-derived tag and refuses a dirty tree, an untagged version, or a `HEAD` off the tagged commit; `.github/workflows/release.yml`, triggered on a pushed bare-version tag, invokes it and attaches the archive to the GitHub Release. Confirmed on a real, scratch-tagged run (`30506726912`): archived file list matched `git ls-tree -r --name-only <tag>` exactly, two builds from the same tag were byte-identical, and the downloaded release asset carried `Cargo.lock`. *(Known gap: `.vscode/` is tracked in this repository, so it legitimately appears in the archive per this requirement's own "exactly the tracked content" clause — see §11 G-34's resolution note.)* |

**PRQ-04 note.** Measured against effective lines of code, **five files
exceed the 500-line "strongly recommended" threshold** — the largest at
762 — and eight exceed the 300-line "consider splitting" threshold. All
but one sit in the interface Worker's rendering layer. This is a
standing deviation rather than an accident, and it SHOULD be either
scheduled for splitting or explicitly accepted with a recorded
rationale, so that the threshold retains meaning.

---

## 11. Conformance gaps

Every gap below was **verified directly against the v0.27.2 source**
for this document, rather than carried over on trust from the review
draft. They are ordered by severity.

Gap identifiers deliberately preserve the numbering of the v0.27.1
data-structure review, so the two documents can be read side by side.
**G-02 is intentionally absent**: the review's second finding was the
absence of multi-tenant structure, which is not a defect but an
unresolved product question, recorded as decision D-1 in §13.

### Blocking

| ID | Requirement | Finding | Consequence |
|---|---|---|---|
| ~~G-01~~ | DR-MIG-01, DR-MIG-02 | ~~The initial migration already defines the audit hash columns and their index; the second migration adds the same columns again with unconditional `ALTER TABLE`. Both changes landed in the same commit — the 0.27.2 release — by amending a migration that had already shipped under tag 0.1.0.~~ | ~~`wrangler d1 migrations apply` stops at the first failure, so `0002` blocks every subsequent migration; nothing after it can ever apply.~~ **Closed 2026-07-28.** |

**Resolution: `sql/0002_audit_hash_chain.sql` deleted; `sql/0001_initial.sql`'s
DDL left unchanged** (its comment block was extended to carry `0002`'s
explanatory prose forward, per rfcs/handoffs/01-migration-applicability.md
Build step 2). This is the second of the two options this entry
originally listed — the first (removing the columns from `0001`) was
rejected because a database already provisioned from the current `0001`
would still fail at `0002`, since `0001` is already recorded as applied
for it. Recorded as [DEC-010](./decision-log.md#dec-010).

Deleting `0002` does not repair a **Class A** database (provisioned from
tag 0.1.0, never re-migrated) — that database's `audit_logs` still lacks
the columns, and Subject 01 could not fix it without touching a
migration that has shipped. A request-time schema assertion
(`db::audit::assert_hash_columns_present`, Build step 4) refuses service
with a named, actionable error rather than silently discarding failed
audit inserts, for as long as such a database exists.

*(Corrected 2026-08-02. This paragraph previously went on to say
Subject 06's migration `0004` "converges all three classes, giving
Class A rows the columns with NULL values" — an unachievable promise,
found by independent review
(`.git-exclude/reviewed/029-subject-06-escalations.md` §5): a single
static SQL statement cannot conditionally copy a column pair that may
or may not exist in the source. DEC-021 (`decision-log.md`) resolved
this the other way — `0004` serves Classes B and C only and is assumed,
not verified, never to meet a Class A database, an assumption accepted
because the migration **fails safe** against one: naming
`prev_hash`/`row_hash` in the copy is exactly what makes it fail at
prepare time if a Class A source lacks them, before any statement runs,
leaving the database untouched and `assert_hash_columns_present` still
the active guard. See §11 G-03's own resolution for the full reasoning
and its T-29a confirmation.)*

Regression coverage: T-01–T-03 and T-01a in
`scripts/check-migrations.sh`, wired into CI as the migration-apply gate
(DR-MIG-05); `db::audit::tests` for the request-time assertion's error
classification. T-01 and T-01a were captured failing against the
pre-fix commit in `.git-exclude/evidence/baseline-01.log` before this fix
landed (NFR-QA-09).

### High

| ID | Requirement | Finding | Consequence |
|---|---|---|---|
| ~~G-38~~ | DR-STO-01, FR-TGT-01, FR-TGT-05, FR-MON-*, FR-INC-03, and every requirement whose acceptance depends on writing a row or reading a paginated list | ~~Binding an `i64` to a D1 statement produces a JS `BigInt`, which D1's bind validation rejects outright~~: `D1_TYPE_ERROR: Type 'bigint' not supported`. `wasm-bindgen` routes `i8 u8 i16 u16 i32 u32` through `JsValue::from_f64` (a JS Number, accepted) and `i64 u64` through `wbg_cast` (a BigInt, refused) — `wasm-bindgen-0.2.122/src/lib.rs`, `integers!` and `big_integers!`. ~~23 binds across 10 statements in 6 modules~~: `db/targets.rs` create (6: `port` :88, `expected_status` :90, `tls_threshold_days` :92, `timeout_sec` :93, `retry_count` :94, `interval_minutes` :95) and update (6: :131, :142, :148, :155, :156, :157); `db/results.rs` insert (3: `status_code` :19, `response_time_ms` :23, `tls_days_left` :37); `db/states.rs` :129,130; `db/incidents.rs` :36, :61; `db/results.rs` :55; `db/audit.rs` :505, :528; `db/retention.rs` :197. Every `… as i32` cast is safe, and every `Option<String>` bind is safe. Reproduced on the write path against real local D1 by the dev team, who also corrected the reviewer's first sweep in three places. | ~~**The highest-severity entry in this register — more severe than G-36.** G-36 broke reads of six tables; this breaks the **core write path and every paginated read**. A target cannot be created or updated on any path; the monitor cannot record a check result carrying a status code, response time or TLS days-left — the highest-frequency write in the system, once per check per target per interval; an incident cannot be resolved; and results, incidents and audit entries cannot be listed.~~ **Not a regression** — it predates every release. **Closed 2026-08-08 — Subject 07c.** Fixed by `i64_to_d1`/`opt_i64_to_d1` (`noye-shared`, beside `bool_from_d1`): constructs the JS Number directly via `JsValue::from_f64`, sidestepping `wasm-bindgen`'s `i64`→`BigInt` path entirely, and **rejects rather than truncates** anything outside `±2^53` (T-194's critical half). Applied at all 23 binds; T-195–T-202 confirmed every affected write and paginated read against real local D1, including **T-200 — `run_cleanup` completing a full pass for the first time in this project's history**, archiving and deleting a real eligible row. T-201 re-ran the bind sweep and found nothing left unconverted. Unblocks 07a's #2/#3/#4, previously blocked first by G-36 then by this gap. Surfaced **G-39** (`db/migration.rs`'s pre-existing truncating `as i32` casts) along the way — reported, not folded in. |
| **G-39** | NFR-MNT-* | `db/migration.rs` binds every `i64`/`Option<i64>` through an explicit `as i32` cast (13 sites, e.g. `Some(p) => JsValue::from(p as i32)` at :283). This is **safe from G-38's `D1_TYPE_ERROR`** — `i32` becomes a JS Number — but it is a silently truncating conversion, which is the shape subject 07c's own guidance forbids introducing. Found and reported, not fixed, by the dev team during 07c's sweep. | **Low.** No live failure: nothing plausibly written to `port`, `expected_status`, `timeout_sec`, `retry_count`, `interval_minutes` or `tls_threshold_days` approaches `i32`'s range. It is a latent-truncation and consistency defect — once 07c introduces a reject-don't-truncate helper, these thirteen sites will be the only place in the codebase converting integers for D1 by a different and weaker rule. Deliberately **not** folded into 07c: bundling a thirteen-site refactor into the fix for the register's most severe gap would make the critical change harder to review (standing rule 5). |
| ~~G-40~~ | NFR-QA-01, NFR-QA-06, NFR-SEC-* | **`noye-gateway`'s 13 WASM-target tests fail, and nothing has ever run them.** `cargo test -p noye-gateway --target wasm32-unknown-unknown` panics at `auth/crypto/digest.rs:94`; the tests cover SHA-256, random generation, base64url and JWT verification — the primitives beneath the audit hash chain, CSRF tokens, session handling and OIDC verification. The failure looks environmental (Web Crypto availability under `run_in_node_experimental`) rather than a defect in the primitives, but **that is a guess, and the point of this entry is that nobody can currently tell.** `.cargo/config.toml` documents the command as though it works. Found by the reviewer while scoping the WASM-test CI job, after G-36 and G-38 established that unexecuted code is where this project's severe defects live. | **Medium.** No evidence the cryptographic code is wrong — the host-side tests that exist pass, and the primitives are thin wrappers over Web Crypto. The defect is that **13 tests over the project's security primitives have never been observed passing or failing on purpose**, so neither the code nor the tests can be relied on. Whether these ever passed is unknown. The new `wasm-tests` CI job deliberately excludes `noye-gateway` until this is diagnosed: a job that is red on arrival gets ignored, which is how a gate stops being one. **Closed 2026-08-11 as classified — subject 07d Step 6.** The answer was *not* environmental: 9 of the 13 tests pass (base64url, jwt_verify ×5, random ×3); the 4 failures are all SHA-256, and the cause is a real defect now tracked as **G-42**. This entry's own summary said "13 fail", which was the reviewer's count from a job-level failure rather than a per-test one. |
| **G-42** | FR-AUTH-01, NFR-SEC-* | **`crypto::sha256()` can never succeed.** `crates/gateway/src/auth/crypto/digest.rs:30` annotates `subtle.digest()`'s resolved value as `Uint8Array` and calls `dyn_into()` on it. `subtle.digest()` resolves to an **`ArrayBuffer`**, and an `ArrayBuffer` is never `instanceof Uint8Array` — the cast cannot succeed in any conforming JS engine. The comment on line 34 describes the correct fix (`Uint8Array::new()` wrapping the buffer) and is unreachable because the annotation above it is wrong. **One call site: `auth/oidc.rs:164`, the PKCE S256 code challenge, on every login initiation.** Classified by the dev team during subject 07d Step 6 (G-40's diagnosis) via a temporary diagnostic that confirmed the constructor name is `ArrayBuffer`. | **High — and it fails *closed*, so it is an outage, not a vulnerability.** `sha256()` returns `Result` and `oidc.rs:166` maps it to an error, so a login attempt **fails with a 500**. It does not produce a weak challenge, a predictable verifier or a bypass; nothing is less secure than intended. **If it reproduces on workerd, OIDC login has never worked in any deployment of this project** — the same shape as G-36 and G-38, unexecuted code hiding a defect nothing ran. Observed under Node (`wasm-bindgen-test`); **not yet observed under workerd**, which subject 07e confirms first via `wrangler dev --local` — that is the production runtime and is inside standing rule 7. Closed by subject 07e. |
| **G-41** | NFR-QA-01 | Reading an `INTEGER` **beyond ±2^53** into a typed `i64` field traps at `worker`'s internal `.unwrap()` (`worker-0.8.5/src/d1/mod.rs:491`) — a hung request with no application-level log, the same unloggable shape as G-36 — rather than returning an error. Message: `invalid type: floating point '9223372036854776000.0', expected i64`. **The value is unrecoverable either way**: D1 hands the column back as a JS Number, so `i64::MAX` arrives as `9.223372036854776e+18` before any Rust code touches it (confirmed by reading the same row as `serde_json::Value`). Found by the dev team during subject 07d's boundary audit, by inserting `i64::MAX` via raw SQL. | **Low, and not reachable from this codebase.** `i64_to_d1` refuses to write beyond ±2^53 (07c); every aggregate read into `i64` is a `COUNT`/`SUM` bounded by row count; and no domain column approaches the limit. The only route in is a direct `wrangler d1 execute` or a hand-written migration — operator action. Unlike G-36 and G-38 there is **no Rust-side fix that recovers the value**; the only question is whether the operator who caused it gets a diagnosis or a hang. A fix would mirror `bool_from_d1` — an `i64_from_d1` visitor that range-checks and errors cleanly — at the cost of an attribute on ~20 fields. Deliberately **not** folded into 07d, which already carries the audit, G-39 and G-40. See **DEC-023** for the boundary limit itself, which is a platform property rather than a defect. |
| ~~G-36~~ | DR-STO-01, NFR-QA-01, and every requirement whose acceptance depends on reading a row | ~~Every `bool` field in a struct D1 deserializes into is backed by an `INTEGER` column, and the `worker` crate's `D1Result::results::<T>()` calls `serde_wasm_bindgen::from_value(…).unwrap()`~~ (`worker-0.8.5/src/d1/mod.rs`). ~~SQLite has no boolean type; D1 surfaces the column as a JS number; `serde_wasm_bindgen` does not coerce a number into a Rust `bool`, and the workspace defines no `deserialize_with`, `serde(from …)` or equivalent anywhere. Seven fields across six structs and six tables: `User.is_active`, `Target.is_disabled`, `CheckResult.is_success`, `MaintenanceWindow.is_active`, `MaintenanceWindow.suppress_notify`, `NotificationChannel.is_enabled`, `RetentionPolicy.archive_to_r2`.~~ Found by the dev team during subject 07a, executing `run_cleanup` against the local D1 runtime for the first time in this project's history. | ~~**The highest-severity entry in this register. The service has never worked against D1 and cannot.** The failure is a **Wasm trap, not a returned error** — so `?` cannot propagate it, no `console_error!` fires, and a real deployment aborts the invocation with no application-level log line. Listing targets, authenticating a user, recording a check result, evaluating a maintenance window and the retention pass all read one of these structs and trap on the first row.~~ **Not a regression** — it predates every release, and reverting removes nothing. It went undetected because all 486 tests exercise pure functions on the host, none constructs a `D1Database`, and until G-01 was closed no database could finish migrating far enough for anyone to try. **Closed 2026-08-03 — Subject 07b.** Fixed by `bool_from_d1`, a `serde::de::Visitor` (`visit_bool`/`visit_i64`/`visit_u64`/`visit_f64`; truthiness `n != 0`; `NaN` rejected as an error rather than read as `true`) applied via `#[serde(deserialize_with = "bool_from_d1")]` to all seven fields. Proved on `User.is_active` alone before the other six, per T-189/T-190/T-191; T-193 confirmed all five reachable tables against real local D1 — this project's first successful typed read against D1. The fix made **G-38** reachable immediately after. |
| **G-37** | NFR-QA-01, NFR-QA-06 | **`noye-core` cannot run tests at the wasm boundary at all.** Its wasm test binary fails to load under Node: `wasm-smtp-cloudflare` references a `cloudflare:`-scheme import, and Node's ESM loader rejects it at module-load time — `ERR_UNSUPPORTED_ESM_URL_SCHEME` — before any test filter is consulted, so no test in the crate can run regardless of which one is requested. Found by the dev team while placing G-36's reproductions, which had to be relocated to `noye-shared` as a result. | **Medium, and structural.** Every test `noye-core` has is host-target, which is exactly the condition that let **G-36** — a defect at the Rust/JS runtime boundary — survive four milestones and a release. The crate holding the D1 access layer, the monitor and the audit chain cannot have a single test that exercises the boundary where its most severe defect lived. It does not break the product; it removes the ability to detect a whole class of defect in the crate most likely to carry one. |
| ~~G-03~~ | FR-AUD-07, FR-AUD-08, DR-INT-04 | ~~The audit actor column is a foreign key to the user table, but system events are written with actor `system`, for which no user row exists. The resulting insert failure is discarded by the caller.~~ | ~~System-originated audit events can be **silently absent**. The chain still verifies, so the loss is undetectable by the integrity check.~~ **Closed 2026-08-02 — Subject 06.** FR-AUD-08's broader "surfaced, not silently discarded" concern survives as G-26 — this closes only the actor-constraint failure mode. |
| ~~G-04~~ | FR-AUD-06, DR-LIF-04 | ~~The default retention policy includes audit rows with a 365-day period, and the cleanup routine deletes them.~~ | ~~Directly contradicts the tamper-evidence design. After the retention period, deletion **breaks the hash chain**, and the integrity check will report the result as damaged.~~ **Closed 2026-07-30 — Subject 04.** |
| G-05 | FR-TGT-08, FR-MIG-08 | The target table requires creator and updater columns, but the shared target model omits them and the import path does not populate them. | Configuration import into an empty database is expected to **fail a not-null constraint**. |
| G-06 | DR-ENT-01, DR-ENT-04, FR-MIG-08 | Import does not create the per-target state row, and thresholds live on that row rather than on the target. | Imported targets are **not monitorable**: state lookup fails, and the configured thresholds are lost in a round trip. |
| G-07 | FR-SUP-07, FR-SUP-08 | The suppression check does not test the suppression flag; the SLA exclusion query tests neither the suppression flag nor the active flag. | A window explicitly marked as non-suppressing **still suppresses notifications**, and deactivated windows still affect SLA. |
| G-12 | FR-SLA-05 | Suppressed time is removed from measured downtime, but the denominator remains the full window length. | Reported SLA does not match the stated definition or the on-screen explanation. |

### Medium

| ID | Requirement | Finding | Consequence |
|---|---|---|---|
| G-08 | FR-SUP-03, DR-INT-06 | Scope is evaluated as a disjunction of target, tag, and global, with no precedence and no exclusivity constraint. | A window naming both a target and a tag applies more broadly than intended. |
| G-09 | FR-TGT-10, DR-ENT-03 | Tags are stored as JSON text and matched with a substring pattern. | A window scoped to `api` also matches `api-v2`; `prod` also matches `production`. **Silent over-suppression.** |
| G-10 | FR-INC-08 | Manual resolution computes and stores duration; automatic resolution does not. | Automatically resolved incidents — the overwhelming majority — are **missing from mean-time-to-recovery**, making the figure misleading rather than merely incomplete. |
| G-11 | FR-INC-03, DR-INT-05 | Single-open-incident-per-target is a property of the application flow, with no database constraint. | Re-entrant scheduling, manual operations, or any future concurrency can produce duplicate open incidents. |
| G-13 | DR-INT-01..03 | Boolean, range, and interval constraints are absent from the schema. | Out-of-range values can enter through import, CLI, or direct database access, bypassing application validation. |
| G-14 | DR-INT-07 | Schema defaults produce space-separated timestamps; the application writes RFC 3339. | Because scheduling and overlap comparisons are string comparisons, mixed formats compare **incorrectly but silently**. |
| G-16 | FR-USR-05, FR-RBAC-07 | The email uniqueness constraint is case-sensitive, and identity resolution leans on email. | Case variation from a provider can create a **duplicate account** for one person. A stable subject identifier column would remove the dependency. |

### Low

| ID | Requirement | Finding | Consequence |
|---|---|---|---|
| G-15 | DR-INT-08, NFR-PERF-04 | Several access paths lack indexes, notably channel-to-target lookups, incident ordering, window overlap, and audit filtering. | Acceptable at present scale; degrades as history accumulates. |
| G-17 | FR-INC-10 | The schema permits an `acknowledged` incident state that no code produces. | Unreachable state invites future divergence. Either implement acknowledgement with its own timestamp and actor, or remove the value. |
| G-18 | FR-NTF-13 | Delivery outcomes are logged to the console only; no delivery record is persisted. | An operator cannot answer "was this incident notified?" after the fact. |
| G-19 | FR-AUTH-03, FR-AUTH-02 | No per-endpoint OIDC override variables exist; endpoint resolution is discovery-only. | A provider that does not publish a discovery document is unsupported. FR-AUTH-02's "any standards-conformant provider" was overstated. |
| G-23 | PRQ-05 | 40 implementation files carry inline `#[cfg(test)] mod tests`; no sibling `tests.rs` exists. | The stated rule is not partially followed but wholly unfollowed. |
| G-24 | PRQ-08, CON-09 | ~~`package.sh` produces a nested archive layout~~ **(archive half closed 2026-07-28, Subject 03a)** and its header comment is Japanese; `ROADMAP.md` carries one Japanese phrase. | ~~Two mechanically checkable process rules are violated while marked satisfied.~~ CON-09 (language) remains open — Subject 34, narrowed to this half. |
| G-25 | — | All six `ROADMAP.md` → RFC links point at `rfcs/NNNN-…` rather than `rfcs/proposed/NNN-…`. `ROADMAP.md` and RFC 0006 also state Slack receives generic JSON, which has been false since before v0.27.2. | Dead cross-references and a stale claim that would send an implementer to reimplement shipped behaviour. |

**G-24 (archive half) resolution.** `package.sh`'s
`--transform 's,^\.,noye,'` removed — this was the sole cause of the
nested layout. Verified by extraction, per Subject 03a's instruction,
not by reading the script: `bash package.sh` followed by `tar x`
lands `Cargo.toml`, `crates/`, `docs/`, etc. directly at the
destination root. Pulled forward from Subject 34 (M5) to Subject 03a
(M0) because releases begin at M0 and every release before the M5 fix
would otherwise have shipped the forbidden layout — a scheduling
defect, not a technical one. Subject 34 retains the Japanese-comment
half (CON-09) untouched, deliberately, per Subject 03a's own "do not
touch" instruction.

### Gaps added 2026-07-28 (independent review)

Verified against v0.27.2 source. These are defects, not deferrals.

| ID | Requirement | Finding | Consequence |
|---|---|---|---|
| ~~G-20~~ | DR-LIF-06, DR-LIF-07, NFR-REL-03 | ~~Retention archives at most one batch (1000 rows) but deletes every eligible row without limit.~~ | ~~**High.** On any pass with more than one batch eligible — the ordinary case for `check_results` — the excess is deleted permanently and unarchived.~~ **Closed 2026-07-28.** |
| ~~G-21~~ | NFR-SEC-09, NFR-SEC-14, NFR-SEC-15 | ~~The shipped `wrangler.toml` sets `NOYE_ENV = "development"` and a literal `GATEWAY_SHARED_TOKEN`; the dev-fallback guard returns early when the environment is `development`, so the control is disabled by the configuration it protects.~~ | ~~**High.** Deploying the repository unmodified yields a permissive environment authenticated by a value published in the repository.~~ **Closed 2026-07-28.** |

**G-20 resolution.** `crates/core/src/db/retention.rs::run_cleanup`
restructured into a per-table loop that selects up to
`RETENTION_BATCH_SIZE` (100) eligible rows, archives that exact batch
when the policy calls for it, then deletes that exact batch by id — the
archived set and the deleted set are now the same query result, never a
separately bounded select paired with an unbounded delete. Deletion
happens per batch, not accumulated to the end of the pass, so a
timed-out invocation is resumable (DR-LIF-07). The string-interpolated
`SELECT` in the old `archive_old_records` is gone; every query binds its
parameters. The bare `_ => continue` for an unrecognized
`retention_policies` table now logs a diagnostic before skipping it.
`RETENTION_BATCH_SIZE`'s value is a reasoned default (D1's documented
per-statement bound-parameter ceiling), not verified against a live D1
instance — no Wrangler/D1 environment was available while implementing
this. Regression coverage: `db::retention::tests` (pure logic — the
eligibility predicate, id extraction) plus a SQL-level reproduction
against local SQLite in `.git-exclude/evidence/baseline-02.log`
(pre-fix) and `.git-exclude/evidence/subject-02-tests.log` (post-fix,
1500 rows across 15 batches, archived_total == deleted_total == 1500).
T-06 and T-07 (forced-failure and resumability across passes) are
guaranteed by the `?`-propagation order in the code but were not
exercised against a live D1/R2 fault injection — see that same evidence
file for the honest accounting.

**Build step 4, added 2026-07-28 after review of the delivered code.**
`run_cleanup` made archiving conditional on `policy.archive_to_r2` but
deletion unconditional, so a policy with `archive_to_r2 = 0` on
`check_results` or `incidents` deleted rows that were never archived —
recreating G-20's consequence through configuration rather than a bug.
`requires_archival` now gates this: for those two classes,
`archive_to_r2 = 0` is treated as a configuration error and the policy
is skipped (reported via `console_error!`, the same pattern already
used for an unrecognized table), never honoured into a delete. Covered
by `db::retention::tests` (T-09a's host-testable half); the end-to-end
skip-and-report path was not exercised against a live D1 environment,
for the same reason as T-06/T-07 above.

**G-21 resolution.** Both `crates/*/wrangler.toml` were renamed to
`wrangler.toml.example` and are `.gitignore`d going forward
(`crates/*/wrangler.toml`); neither carries a value for
`OIDC_CLIENT_SECRET` or `GATEWAY_SHARED_TOKEN` — only instructions
pointing at `wrangler secret put` (deployment) and `.dev.vars` (local
development, also git-ignored). Gateway's template ships `NOYE_ENV =
"production"`; Core's does not set `NOYE_ENV` at all — audit finding
F-3 (`.git-exclude/reviewed/015-audit-subject-03-m0.md`) caught that
Core's template originally still set it despite Core never reading the
variable, which external design §9.2 already scoped to Gateway only. In
both
`env_check.rs` files, `check_no_leaked_dev_fallbacks` no longer branches
on `Environment::is_development()` at all; the decision was extracted
into a pure `find_leaked_fallback(observed)` function that takes no
environment parameter, so there is no remaining code path that could
reintroduce the bypass. `crates/core/src/env_check.rs`'s `Environment`
type was removed entirely — Core had no other consumer of it (verified
by `grep`) once the fallback check stopped needing it, and the gateway's
copy keeps its own for cookie-strictness reasons that don't apply to
Core. Regression coverage: `find_leaked_fallback` unit tests in both
crates' `env_check/tests.rs` (T-11, T-15); T-10 (file/`.gitignore`
state) and T-11 were captured failing against the pre-fix commit in
`.git-exclude/evidence/baseline-03.log` — T-11 could not be executed
live (no D1/Workers `Env` is constructible on the host target), so that
half is a direct quote of the pre-fix source rather than a captured
command, per the evidence README's rule against asserting untested
outcomes. T-12/T-13 (guard: refused regardless of `NOYE_ENV`) and T-14
(Gateway → Core auth with a generated token) hold by the same
"no environment parameter" argument but were likewise not exercised
against a live Workers runtime.
| **G-22** | FR-MIG-08, DR-ENT-01 | Configuration import uses `INSERT OR REPLACE INTO targets`, which resolves conflicts by deleting the row and therefore fires `ON DELETE CASCADE` on states, results, incidents and attachments. | **High.** A `replace` import silently destroys operational history while reporting success. Closing G-05 alone converts today's loud `NOT NULL` failure into this silent one — they must be fixed together. |
| ~~G-26~~ | FR-AUD-08, FR-AUD-11 | ~~All audit call sites in the API layer are `let _ = db::audit::log(…).await` or `let _ = db::audit::log_system(…).await` -- seventeen, not the originally stated nineteen (two named sites were `monitor/engine.rs`'s `log_system` calls, not additional `api/` sites; corrected during subject 07's pre-flight).~~ | ~~**High.** Not only system events (G-03): every admin mutation can complete with no audit row, and the chain still verifies because it covers only rows that were written.~~ **Closed 2026-08-02 — Subject 07.** |
| **G-27** | FR-SUP-03, FR-TGT-10 | The stored tag is concatenated into the **pattern** side of `LIKE`, so a tag containing `%` or `_` acts as a wildcard. A window scoped to `%` suppresses every tagged target. | Medium. Compounds G-09: that is prefix collision between honest tags, this is metacharacter injection. |
| **G-28** | FR-INC-10 (parallel) | `target_states.current_status` permits `degraded` and `maintenance`, which nothing writes, yet `db/targets.rs` counts both for the dashboard status breakdown. | Medium. Two of four breakdown categories are structurally always zero. Same class as G-17, on a different table, with live query code depending on it. |
| **G-29** | FR-INC-02, FR-SLA-06 | `incidents.created_by` is set at open and overwritten at resolve. | Medium. The incident CSV's `created_by` column means "opener" for open rows and "resolver" for resolved ones. |
| ~~G-32~~ | NFR-SEC-10, NFR-QA-06 | ~~The CI dependency-scan job invokes `cargo audit --locked`; cargo-audit 0.22.2 has no such flag and exits 2.~~ | ~~**The vulnerability scan does not run.**~~ **Closed 2026-07-29 — confirmed in a real Actions run.** |
| **G-31** | FR-MIG-04, FR-MIG-06, FR-MIG-08 | `include_users` defaults to **off**, so the default export carries no users — but `targets.owner_id` and `notification_channels.owner_id` are `NOT NULL` with a foreign key to `users(id)`. Import performs no reference validation before writing. | **High.** The default export cannot be imported into a fresh deployment, which is the primary stated use case for the configuration document. It fails with a raw constraint error rather than a validation report, contradicting FR-MIG-06's "all errors in one pass". |
| ~~G-30~~ | FR-AUD-03, FR-AUD-04, FR-AUD-05 | ~~The chain's order is insertion order; `verify_chain` reconstructs order by sorting stored columns (`ORDER BY action_time ASC, id ASC`).~~ | ~~High.~~ **Closed 2026-07-31 — confirmed against real host tests, including a real hang-then-fix reproduction for the totality defect found during review.** |
| ~~G-34~~ | PRQ-14, NFR-SEC-09 | ~~`package.sh` builds the archive with `tar … .` over the **working directory**, excluding only `target/`, `Cargo.lock`, `dist/` and `.git/`. Everything else on disk ships, tracked or not.~~ **Closed 2026-07-30 — confirmed on a real, scratch-tagged Actions run.** |
| ~~G-33~~ | NFR-QA-04, NFR-QA-05, NFR-QA-06 | ~~`.github/workflows/ci.yml:42` calls `rustup toolchain install 1.91 --profile minimal --component rustfmt clippy` — `--component` takes a comma-separated list, and the space-separated form parses `clippy` as a second, invalid toolchain name. The "Format, lint, check" job fails at this step, before Format, Clippy, or Cargo check ever run.~~ **Closed 2026-07-29 — confirmed in a real Actions run.** |
| ~~G-35~~ | PRQ-15 | ~~`.github/workflows/release.yml:50` publishes the release with `gh release create … --generate-notes`, which is GitHub's automatic commit and pull-request summary.~~ | ~~High at 0.28.2, not before.~~ **Closed 2026-07-31 — confirmed on real, scratch-tagged Actions runs.** |

**G-32 resolution.** `.github/workflows/ci.yml`'s `audit` job corrected
from `cargo audit --locked` (rejected by cargo-audit 0.22.2 with exit
2, before any scanning) to `cargo audit`. **Confirmed in a real GitHub
Actions run** (2026-07-29, PR #2, run `30455673409`): the job fetched
1173 advisories and scanned 224 crates — a genuine scan, not the exit-2
failure the `--locked` form produced. `rfcs/handoffs/
evidence/subject-03b-tests.log`. Discovered alongside — and unrelated
to — RUSTSEC-2026-0190 (`anyhow`), fixed by `cargo update` under
Subject 03/M0 release prep. `rfcs/handoffs/
evidence/README.md` and `rfcs/handoffs/36-release-rehearsal.md`
corrected to the same invocation.

**G-33 resolution.** `.github/workflows/ci.yml`'s `check` job
corrected from `rustup toolchain install 1.91 --profile minimal
--component rustfmt clippy` (space-separated — parses `clippy` as an
invalid second toolchain argument, failing before Format/Clippy/Check
ever run) to `--component rustfmt,clippy` (comma-separated, as
`rustup` requires). **Confirmed in a real GitHub Actions run**
(2026-07-29, PR #2, run `30460161440`): all 5 jobs pass, including
"Format, lint, check" — the first time in this project's history that
job has completed (every prior run, back to baseline commit `5de978d`,
failed at the same toolchain-install step). Additionally confirmed the
gate fails on a real violation, not just passes on a clean tree: a
scratch branch (`scratch/t170-lint-violation`, discarded after
confirmation) introduced a deliberate `clippy::bool_comparison` /
rustfmt violation and run `30460920132` shows "Format, lint, check"
failing at the "Format" step while the other 4 jobs stay green.
`.git-exclude/evidence/subject-03c-tests.log`.

**G-34 resolution.** `package.sh` corrected from `tar … .` over the
working directory with a maintained exclude list, to `git archive`
over the git tag matching `[workspace.package].version` — exactly the
tracked content of that commit, with no exclude list to maintain
(DEC-019 additionally drops the `Cargo.lock` exclusion). It refuses to
run against a dirty working tree, a version with no matching tag, or a
`HEAD` not at the tagged commit. `.github/workflows/release.yml`
added, triggered on a pushed bare-version tag, to invoke `package.sh`
and attach the archive to the GitHub Release — production moves from a
local, human-run script to an observed workflow run, the same fix
shape as G-32 and G-33. **Confirmed on a real Actions run** (2026-07-30,
scratch tag `99.99.99` on a scratch branch, run `30506726912`): the
release was created with both `noye-project-v99.99.99.tar.gz` and
`noye-README-v99.99.99.md` attached; the downloaded archive's file
list matched `git ls-tree -r --name-only <tag>` exactly (208 files,
zero diff) and two local builds from the same tag were byte-identical.
Scratch tag, branch, and release deleted after confirmation.
`.git-exclude/evidence/subject-03d-tests.log`.

**G-04 resolution.** `sql/0001_initial.sql` seeded a 365-day
`audit_logs` retention policy, and `retention.rs` had a matching
deletion arm — after 365 days the deletion broke the hash chain, the
product destroying its own evidence on a schedule. Fixed two ways, per
Subject 04's "Why both" (same pattern as Subject 03): migration
`sql/0003_audit_retention_exemption.sql` deletes the seeded policy row
(idempotent), and a new `is_non_expiring` guard in `run_cleanup`
refuses to delete from `audit_logs` regardless of any policy row
present — consulted first, before eligibility is even checked, and the
`audit_logs` arm is removed from `eligibility_where_clause` entirely
so the code no longer knows how to select its rows for deletion even
if the guard were bypassed. `is_non_expiring` takes only the table
name, never the policy row's other fields, so a hand-reinserted policy
row (any `retention_days`, any `archive_to_r2`) cannot change the
outcome — the closest host-testable proxy available for "a full pass
deletes zero audit rows" (T-16) without a live D1/Wrangler environment
(same constraint as Subject 02's `RETENTION_BATCH_SIZE`). T-17 added to
`scripts/check-migrations.sh` (no `audit_logs` row in
`retention_policies` after migration), confirmed must-fail-first by
temporarily removing `sql/0003` and re-running the gate.
`.git-exclude/evidence/baseline-04.log`,
`.git-exclude/evidence/subject-04-tests.log`.

**G-35 resolution.** `.github/workflows/release.yml` published with
`gh release create … --generate-notes` — GitHub's auto-generated
commit/PR summary — instead of the curated `CHANGELOG.md` entry
`RELEASE.md` § Release notes requires, and a tag with no dated
changelog section still published successfully with a thin
auto-generated body. Fixed two ways, per the handoff's "both halves,
or neither" (same pattern as Subjects 03 and 04): new
`scripts/changelog-section.sh <version>` extracts the exact body of
`## [<version>] — <date>`, exiting non-zero (with a message on stderr)
when the section is missing or empty; `release.yml` runs it first,
before the archive is built or anything is published, writing to
`RUNNER_TEMP` rather than the checkout (an untracked file inside the
repo would trip `package.sh`'s own dirty-tree refusal from Subject
03d — found and fixed during verification, not assumed). `gh release
create`'s `--generate-notes` replaced with `--notes-file`; the
already-exists branch gained `gh release edit --notes-file` alongside
its `--clobber` upload, so a re-run converges on the same notes as a
first run. **Confirmed on real, scratch-tagged Actions runs**: a
section with a distinctive marker published that marker verbatim
(T-178); a tag with no section failed at the extraction step with no
release created (T-180); re-running the workflow against an existing
release left the same notes (T-183); a `0.28.10`-shaped collision
against a `0.28.1` query does not match (T-182, via a constructed
fixture). Part 2, bundled per the handoff's documented exception to
standing rule 5: `actions/checkout` and `actions/cache` bumped off
v4 (Node 20, forced onto Node 24 with a deprecation warning) to v7/v6
respectively, confirmed by the annotation's absence from a real run,
not by reading either action's release notes.
`.git-exclude/evidence/baseline-04a.log`,
`.git-exclude/evidence/subject-04a-tests.log`.

**G-30 resolution.** `verify_chain` recovered the chain's order by
`ORDER BY action_time ASC, id ASC` — not sound, since `action_time` is
second-resolution and `id` is a random UUID, so no sort over
`(action_time, id)` is monotonic with insertion. Twenty rows written
within one second verified clean in 0/2000 simulated runs; a two-row
configuration import, ~51%. A first specified fix (matching the
tiebreaks) was caught as defective before being issued — see
`.git-exclude/reviewed/025-subject-05-defective-fix.md` — because it
addressed which row is chosen as head, not the new row's own random
sort position. **Fixed by reading order from the chain's own links
instead of recovering it by sorting (DEC-020):** `verify_chain` walks
`prev_hash → row_hash` from `GENESIS_HASH` via a new pure function,
`walk_chain`, indexing rows by `prev_hash`; the `ORDER BY` on the fetch
is no longer load-bearing for correctness (T-21). Four classes, not
three — `orphaned` (carries hashes, never reached from genesis) kept
distinct from `tampered` (reached, content doesn't match), so a
deletion's unreachable successors are never named as themselves
altered (T-22/T-23, both confirmed to name the correct row, not merely
detect *something*).

`current_head_hash` derives the head from the same walk — one code
path for chain order, not two that can disagree, which is how G-30
happened. This closes a second escalation: the handoff's first
specified replacement for `current_head_hash` (the row whose
`row_hash` is no other row's `prev_hash`) was reproduced against real
SQLite and found to return two rows after an *ordinary* mid-chain
deletion (T-22's own scenario), not only a genuine fork — an audit
write following any deletion would have been refused under that
design. See
`.git-exclude/review-request/015-subject-05-escalation-tail-query-fork-ambiguity.md`
and the ruling in
`.git-exclude/reviewed/027-subject-05-ruling-and-defect.md`. The head
is now the last row the walk *reached*, not the table's true latest
row — chaining onto an unreachable true-latest row would orphan every
row written from then on. A fork at write time does not refuse the
write (an integrity control must not be convertible into a kill
switch by anyone who can insert one row); it logs at error level and
continues on the same deterministic tiebreak the read path uses
(T-23d).

A third finding, from the same review round: `walk_chain` as first
built did not terminate on a `prev_hash → row_hash` cycle — confirmed
by disabling the fix and hanging the real test under `timeout`
(exit 124), not reasoned about. `audit_logs` may contain rows `log()`
did not write, so every function reading it must be total over
arbitrary content; the walk now refuses to revisit an already-reached
row and reports a cycle distinctly from ordinary tampering (T-23c).
No stored hash is rewritten anywhere in this subject — following the
links repairs the reading, not the data.

A fourth finding, a round later: that cycle fix itself double-classified
the row where the loop closes — once on its first visit, once again as
a `TamperedRow` naming the cycle — inflating the tampered count with a
duplicate id and violating FR-AUD-05's "every row in exactly one class"
on its way to fixing G-30. `ChainVerification` gained `cycle_at:
Option<String>` instead: the looping row's id, reported once, not a
fifth class. New standing guard `assert_partition` (T-23e), called by
every `walk_chain` test, asserts the four classes plus legacy sum to
`total_rows` exactly and no id is double-counted — the property none of
the first eight tests checked, which is how the duplicate passed a full
review round.
`.git-exclude/evidence/baseline-05.log`,
`.git-exclude/evidence/subject-05-tests.log`.

**`.vscode/` resolved, not a gap.** `.vscode/settings.json` and
`.vscode/extensions.json` are tracked in this repository, so they
legitimately appear in every release archive — `git archive` includes
all tracked content by design, and PRQ-14 itself defines correctness
as "exactly the tracked content of the tagged commit." The tension was
in `rfcs/handoffs/03d-release-archive-source.md`'s own T-171 ("no path
under … `.vscode/` …"), which listed what had appeared in the old,
defective archive without distinguishing "was on disk" from "should
not ship." Resolved by amending T-171 to name only the untracked set
(`.git-exclude/`, `.claude/settings.local.json`) and adding T-171a as a
positive guard: the archive **must contain** `.cargo/config.toml` and
`.vscode/settings.json`, so untracking repository content to satisfy a
test would itself now fail one. No gap number was ever assigned; none
needed.

**G-03 resolution.** `audit_logs.actor_id` was `NOT NULL` with a foreign
key to `users(id)`; `log_system` writes the sentinel actor `"system"`,
for which no user row exists, so the insert failed and the caller
discarded the result — system-originated audit events could be
**silently absent**, and the chain still verified because it covers
only rows that were written. Confirmed against real D1 before fixing
anything (`wrangler d1 execute --local`, Step 0): `PRAGMA foreign_keys`
defaults to `1`, and the insert is refused — the obvious `sqlite3`
reproduction gives the opposite, wrong answer, because bare `sqlite3`
defaults that pragma to `0` per connection.

Fixed by the standard SQLite table-rebuild (`sql/0004`): a replacement
`audit_logs` with no foreign key on `actor_id`, `CHECK (actor_id != '')`
in its place, every row copied across with an explicit column list. The
actor is now a snapshot captured at write time (id and, where known,
email), not a live reference, so a later deactivated or renamed user
cannot invalidate history that already happened (DR-INT-04, T-28).

**Scope, per DEC-021 (`docs/src/decision-log.md`):** this migration
serves Class B and Class C databases — `audit_logs` already carrying
`prev_hash`/`row_hash` — and copies those columns directly and
unconditionally. Whether any **Class A** database (provisioned from tag
0.1.0, predating the hash-chain columns) still exists in the wild was
never verified: doing so would have required querying a real,
credentialed Cloudflare D1 database, which standing rule 7
(`rfcs/handoffs/README.md`) forbids regardless of how narrow the query.
Class A is therefore *assumed* absent, not confirmed absent — an
assumption DEC-021 accepts because the migration **fails safe** against
it: naming `prev_hash`/`row_hash` in the copy is exactly what makes the
migration fail at prepare time, before any statement runs, if those
columns don't exist. T-29a confirms this against a real 0.1.0-vintage
fixture — the migration refuses with `no such column: prev_hash`, and
the database is left completely untouched (no partial rename, no
leftover scratch table). Reproducing that "untouched" half correctly
took a second wrong-answer trap of the same shape as Step 0's: bare
`sqlite3 file < script.sql` does **not** abort a script at its first
error the way D1's real migration application does — it prints the
error and keeps executing the remaining statements, so the naive
version of this test passed on a false premise (the rename completed
anyway). Wrapping the migration in an explicit transaction and running
`sqlite3 -bail` reproduces the real atomic-or-nothing behaviour without
needing D1 itself.

T-25/T-29c confirm every classification-relevant column, including the
hash columns themselves, survives the rebuild byte-for-byte — what
subject 05's deterministic, link-based classification (DEC-020) makes a
real guard rather than a coin toss, since before that fix "identical
before and after" was not even well-defined. T-24 and T-29 confirm the
concrete failure this gap named: a `log_system`-shaped insert, and
specifically `monitor/engine.rs`'s two real call sites
(`status_down`/`status_up`), fail before `sql/0004` and succeed after.
T-29b confirms a NULL-hash row still classifies as legacy, never
tampered or orphaned — the property that would have mattered had a
Class A migration ever produced one, and is worth guarding on its own
terms regardless, since Class B/C legacy rows exist for the same
structural reason.
`.git-exclude/evidence/baseline-06.log`,
`.git-exclude/evidence/subject-06-tests.log`.

**G-26 resolution.** Every `db::audit::log`/`log_system` call site read `let
_ = ... .await`, discarding the result unconditionally — seventeen of
them, not the originally stated nineteen (pre-flight found two of the
named sites were `monitor/engine.rs`'s `log_system` calls, not
additional `api/` sites; `rfcs/handoffs/07-audit-write-surfacing.md`'s
own count was corrected before Build started). A transient D1 failure
on any of them produced a completed mutation with no audit row, and the
hash chain still verified — it covers only rows that exist (DEC-011
already named this consequence when it decided the failure policy).

Fixed with three helpers in `db/audit.rs`, per the handoff's
anticipation that call sites without an HTTP response might need a
different shape. `log_or_report` (takes a `Caller`, returns `bool`,
**`#[must_use]`**) is the "attended" case — the fourteen `api/` sites
that have a successful response to attach a warning to. Two
"unattended" siblings return nothing, for sites where there is nothing
further to do with an outcome: `log_system_or_report` (no `Caller`)
for the two `monitor/engine.rs` sites, which run from the cron-driven
monitor with no response at all, and `log_or_report_unattended` (takes
a `Caller`) for `channels.rs`'s `send_test` error branch, which already
returns `Err(e)` for an unrelated failure (the test notification itself
failed to send) and so has no successful response either.

`#[must_use]` on `log_or_report` was a required correction to this
subject's first round: a bare `bool` return is silently discardable
under `-D warnings`, which is exactly how G-26 happened in the first
place, and `send_test`'s error branch first shipped discarding it as a
bare statement — more invisible than the `let _ =` T-35 greps for,
since a bare statement matches no discard pattern at all. Moving that
one site onto `log_or_report_unattended` instead means no call site
anywhere discards an audit outcome, and the compiler — not a census —
is what enforces it. `scripts/check-audit-surfacing.sh` (T-31) still
greps for a 1:1 pairing between `log_or_report` and
`api::with_audit_outcome` across every `api/` file, with no exception
left to name.

All three helpers log at error level on failure — resource type,
resource id, action type, and the actor (`"system"` for
`log_system_or_report`) — **never** `previous_value`/`new_value`. This
is enforced by the signature, not by discipline: `audit_failure_log_line`,
the pure formatter all three call, has no parameter to carry a changed
value through even if someone wanted to (T-33, T-34).

The fourteen `api/` sites with a successful response route their
`log_or_report` result through a new `api::with_audit_outcome(resp,
recorded)`, which attaches `X-Audit-Warning: 1` when `recorded` is
false — a no-op otherwise.

The Gateway relays the same header on its own response
(`core_client::AuditChecked<T>`/`bool` return types carry it through
`with_audit_warning`), and the browser-side script for every mutating
page that has one — channels (create/update/delete/attach/detach/test),
maintenance-adjacent user management, and configuration import/export —
renders `AUDIT_WARNING_MESSAGE` **alongside** the existing success
text, never in place of it, per `external-design.md` §4.5's required
copy and ordering (T-32; `format_retry_after_hint`'s "Rust mirror,
pinned by a test" pattern, not a literal interpolation into the JS).
Target and maintenance creation have no browser UI at all (API-only,
by design) — the response header is still set for any direct API
caller, but there is no script to update.

T-30 (a forced audit failure still lets the mutation complete) and T-36
(the success path is unchanged) are argued from control flow, not
fault-injected: `log_or_report`/`log_system_or_report` return a plain
value, never a `Result` a caller could propagate with `?`, and every
call site performs the business mutation before calling either — there
is no path by which an audit failure could abort or undo it. Confirmed
by inspection of all seventeen call sites, not by exercising a live D1
failure (same environment constraint as Subject 04's T-19).
`.git-exclude/evidence/baseline-07.log`,
`.git-exclude/evidence/subject-07-tests.log`.

### Remediation order

Amended 2026-07-28 to fold in G-19…G-29 and the closed decisions.

| # | Gaps | Milestone | Rationale |
|---|---|---|---|
| **0** | **G-20, G-21** | M0 | Unbounded silent data loss, and a deployment whose default configuration is permissive and authenticated by a published value. Neither is in the original register; both outrank it |
| **1** | **G-01** | M0 | Fresh deployment is broken; nothing else can be validated on a clean database |
| **2** | **G-04, G-03, G-26, G-30** | M1 | The audit trail is the system's evidentiary basis. Fix self-deletion, the actor constraint, the discarded write results **and** the writer/verifier ordering disagreement together — any one alone leaves the trail unreliable. G-30 additionally makes the integrity check produce false positives, which erodes the control faster than a silent gap does |
| **3** | **G-05, G-06, G-22, G-31** | M2 | Configuration import is unsound in four independent ways. **One change**: closing G-05 alone converts a loud failure into G-22's silent history destruction, and G-31 means the *default* export is not importable at all |
| **4** | **G-07, G-08, G-09, G-27, G-12** | M2 | Suppression and SLA diverge from what the interface tells the operator |
| **5** | **G-10, G-11, G-28, G-29** | M2 | Incident correctness and unreachable state values |
| **6** | **G-13, G-14, G-15, G-16, G-17, G-19, G-18** | M2 / M5 | Constraint hardening and observability. Safe to treat as final now that DEC-008 rules out a tenant column |
| **7** | **G-23, G-24, G-25** | M5 | Process, packaging and documentation debt |

Groups 0–4 produce **wrong answers or lost data** rather than missing
features, which is why they precede the rest.

Each closed gap acquires a regression test that fails against the
pre-fix behaviour (NFR-QA-09), which is a merge condition from v0.28.0
onward — not an aspiration. When a gap closes, its entry above is
**struck, not deleted** (§15).

---

## 12. Deferred scope

Accepted requirements with a decided deferral. Each is specified in
`rfcs/proposed/`; the roadmap records why it waits.

| Requirement | Deferred item | Waiting on |
|---|---|---|
| FR-UI (theme) | Manual light/dark/system theme override | Operator demand for an OS override |
| NFR-A11Y (enhanced) | High-contrast preset beyond AA | Demonstrated need; AAA maintenance cost is the objection |
| FR-AUD-09 | Off-system audit mirror | A compliance requirement for external retention |
| FR-OPS-07 | Automated-client challenge on login | Observed attack volume exceeding the rate limit |
| FR-AUTH-10 | Failed-login audit recording | Attribution design for unverified identities |
| FR-NTF-12 | Native Slack formatting | Operator preference for richer rendering |

Deferred without an RFC, deliberately:

| Item | Reason |
|---|---|
| Queue-based fan-out for the monitor | Needed only beyond roughly a thousand targets; designing before the requirement is concrete would be speculative. Requires serialized audit writes as a prerequisite. |
| HTML / multipart email bodies | No operator has requested it; plain text is sufficient for short state-change alerts. |

---

## 13. Open decisions

These cannot be resolved by an implementer and require a product
decision. Each blocks or reshapes requirements above.

### ~~D-1 — Is multi-tenancy in scope?~~ · **CLOSED 2026-07-28**

**Resolution: single tenant per deployment.** Recorded as
[DEC-008](./decision-log.md#dec-008). CON-08 moves to `Implemented`. No
tenant table and no tenant column will be introduced; the schema
constraints added in the hardening phase are therefore final.

Re-evaluate only if a concrete operator requirement appears for two
tenants sharing one Cloudflare account *and* unable to run two
deployments — and revisit **before** any further schema work, not
after.

*Original text retained below for traceability.*

> The current schema is single-tenant throughout: no tenant table, no
> tenant column on any entity. Project material has at points described
> multi-tenant management as a capability, while the architecture
> documentation states that tenants are separated by running separate
> deployments.
>
> These cannot both stand. Two coherent resolutions:
>
> - **Single-tenant per deployment** — record it as a product decision,
>   state it in the README and the UI, and close the question.
> - **Multi-tenant within one deployment** — introduce tenant and
>   membership tables and add a tenant column to targets, channels,
>   attachments, windows, incidents, check results, and audit rows.
>
> If multi-tenancy is wanted at all, it SHOULD be done **before** the
> gaps in §11 are closed, not after: retrofitting a tenant column through
> a hardened schema costs considerably more than adding it while the
> schema is already being corrected.

### D-6 — Role model · **CLOSED 2026-07-28**

**Resolution: exactly two roles, `admin` and `member`**, with `member`
scoped to targets they own. Recorded as
[DEC-009](./decision-log.md#dec-009). The four-role capability ladder
proposed by the parallel UI mockup is rejected, and with it the
on-call tier — consistent with the §2.4 non-goal. FR-RBAC-01 stands
unchanged.

### ~~D-2 — Does SLA need to distinguish "suppress notifications" from "exclude from SLA"?~~ · **CLOSED 2026-07-28**

**Resolution: split into two independent flags.** Recorded as
[DEC-013](./decision-log.md#dec-013). `maintenance_windows` gains
`exclude_from_sla` alongside the existing `suppress_notify`, defaulting
to 1 so existing rows keep today's intended behaviour. The interface
presents three named situations — planned maintenance, known external
outage, expected noise — rather than two unexplained checkboxes (see
FR-SUP-13). FR-SUP-03 is restated as an exclusivity rule and FR-SLA-09
is added for the fully-excluded-window case.

Scheduled for M2 (Phase 3), alongside gaps G-07, G-08, G-09, G-12 and
G-27, which share the same two queries — see §11's remediation group 4.

Re-evaluate only if a third axis of window behaviour is proposed; two
was sufficient to express every case named against the requirements at
this scale.

*Original text retained below for traceability.*

> Today one flag governs both. An operator may reasonably want a window
> that silences alerts without forgiving the downtime — for example
> during a known third-party outage. If both behaviours are wanted, they
> need independent flags, and FR-SUP / FR-SLA must be split accordingly.
> Resolving this alongside G-07 and G-12 avoids reworking the same
> queries twice.

### D-3 — Is the multilingual requirement live? (blocks NFR-I18N)

The development instructions require multilingual GUI support. Nothing
implements it and no RFC tracks it. It should either become an RFC in
`rfcs/proposed/` or be formally withdrawn with a recorded reason.

The decision has architectural weight: string externalization is
substantially cheaper to introduce before further UI work than after,
and it interacts with the contrast-pinning tests, which assume
fixed-length English labels in places.

### D-4 — Should incident acknowledgement exist?

The schema permits an acknowledged state that nothing produces (G-17).
Either implement it — with acknowledgement time and actor, and a
defined interaction with notification suppression — or remove the value
from the constraint.

### ~~D-5 — Should the release archive contain the dependency lockfile?~~ · **CLOSED 2026-07-29**

**Resolution: yes, it carries `Cargo.lock`.** Recorded as DEC-019,
superseding the second half of DEC-006. `git archive` includes it by
default since it is tracked, so no exclusion machinery is needed.

*Original text retained below for traceability.*


The lockfile is committed for reproducible CI but excluded from the
release archive, so recipients re-resolve dependencies. This is
defensible but was never ratified as a decision. Ratify or change it.

---

## 14. Assumptions

Stated so that a reader who disagrees knows exactly which requirement
to challenge.

| ID | Assumption | Requirement affected if false |
|---|---|---|
| A-01 | The identity provider is trusted; its compromise is out of scope. | All of §5.1, §5.3 |
| A-02 | The platform's internal service binding cannot be intercepted; the shared secret is defence in depth, not the primary control. | NFR-SEC-02 |
| A-03 | Operator count stays below roughly one hundred concurrent sessions, so session enumeration completes in one page. | FR-AUTH-08 |
| A-04 | Target count stays in the low hundreds, so one sweep completes within one interval. | NFR-PERF-01, FR-MON-03 |
| A-05 | There is a single writer to the audit chain. | FR-AUD-03, NFR-REL-05 |
| A-06 | Operators and administrators are the same small group; self-service member workflows are not required. | FR-RBAC-04 |
| A-07 | An attacker cannot observe timing of local development processes. | The documented advisory suppression (NFR-SEC-11) |
| A-08 | Deployment is to Cloudflare; portability is not required. | CON-03, DR-STO-* |

---

## 15. Document maintenance

- This specification is versioned with the product. When a requirement
  changes, the entry is amended and the change noted in the changelog.
- When a gap in §11 is closed, the affected requirement's status moves
  to `Implemented`, the gap entry is struck rather than deleted, and a
  regression test is added (NFR-QA-09).
- When an open decision in §13 is resolved, the decision is recorded in
  the decision log with its rationale and re-evaluation criteria
  (NFR-MNT-04), and the dependent requirements are updated in the same
  change.
- Requirement identifiers are never reused. Withdrawn requirements are
  marked, not removed.
