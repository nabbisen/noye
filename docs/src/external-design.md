# Noye — External Design Specification

**Baseline version:** v0.27.2, amended 2026-07-28 for v0.28.0
**Design phase:** External design (basic design)
**Language:** English (project working language)

**Amendment note (2026-07-28).** §9.3 amended and §9.5 added: the
configuration surface moves from a tracked, deployable `wrangler.toml`
to a template the operator copies. No configuration key is renamed, so
no deployed environment breaks.

---

## 1. About this document

### 1.1 Position in the development workflow

The project mandates the sequence:

```
Requirements → External design → Internal design → Program design → Implementation → Test
      ↑              ↑
requirements.md   this document
```

**External design defines everything an outside observer can see or
touch**: screens, URLs, request and response payloads, outbound message
formats, file formats, configuration keys, and observable error
behaviour. It stops at the component boundary.

**It deliberately does not define**: module decomposition, function
signatures, database schema internals, algorithm choices, or crate
layout. Those belong to internal design. Where this document names a
component, it does so only to describe an interface *between*
components, never to prescribe what happens inside one.

### 1.2 Relationship to the requirements

Every interface here traces to one or more requirements in
`requirements.md`. The traceability table is §12. Where an interface
exists that no requirement demands, or a requirement demands an
interface that does not exist, §13 records it.

### 1.3 Verification basis

The interface contracts in this document were **read directly from the
v0.27.2 source**, not reconstructed from earlier design drafts. This
matters: three earlier documents describe interfaces that the shipped
system does not have, and one describes as "not implemented" a feature
that is in fact implemented. Those corrections are consolidated in
§13.2.

### 1.4 Conventions

| Notation | Meaning |
|---|---|
| `:name` | Path parameter |
| `?name=` | Query parameter |
| **admin** / **member** / **any** / **none** | Minimum role required |
| **CSRF** | Endpoint requires a valid synchronizer token |
| `I-nn` | External interface identifier (stable) |
| `S-nn` | Screen identifier (stable) |

---

## 2. System context

### 2.1 Context diagram

```
                    ┌──────────────────────────┐
                    │  Identity Provider       │
                    │  (external OIDC service) │
                    └────────────▲─────────────┘
                                 │ I-04
                                 │ discovery, authorization,
                                 │ token exchange, JWKS
                                 │
   ┌──────────┐   I-01 web UI    │
   │ Operator ├──────────────────┤
   │ (browser)│   I-02 HTTP API  │
   └──────────┘                  │
                          ┌──────┴───────┐
   ┌──────────┐  I-03     │              │        ┌──────────────────┐
   │ External ├──────────►│   Gateway    │        │ Monitored        │
   │ uptime   │  /healthz │  (public)    │        │ endpoints        │
   │ checker  │           │              │        │ HTTP/TCP/SMTP/TLS│
   └──────────┘           └──────┬───────┘        └────────▲─────────┘
                                 │                         │
                                 │ I-07 Service Binding    │ I-05 probes
                                 │ (private, token-guarded)│
                                 │                         │
                          ┌──────▼───────┐                 │
   ┌──────────┐  I-11     │              ├─────────────────┘
   │ Platform ├──────────►│    Core      │
   │ scheduler│  1/minute │  (private)   ├─────────┐ I-06 notifications
   └──────────┘           └──────┬───────┘         │
                                 │                 ▼
                          ┌──────┴──────┐   ┌──────────────────────┐
                          │ D1 / KV / R2│   │ Webhook / Slack /    │
                          └─────────────┘   │ SMTP relay           │
                                            └──────────────────────┘
```

### 2.2 External entities

| Entity | Direction | Interface | Notes |
|---|---|---|---|
| Operator (browser) | inbound | I-01, I-02 | Authenticated human user |
| External uptime checker | inbound | I-03 | Unauthenticated; monitors Noye itself |
| Identity provider | outbound | I-04 | Any standards-conformant OIDC service |
| Monitored endpoints | outbound | I-05 | The systems under observation |
| Notification destinations | outbound | I-06 | Webhook receivers, Slack, SMTP relay |
| Platform scheduler | inbound | I-11 | Fires the monitoring sweep |
| Deployment operator | configuration | I-10 | Sets variables, secrets, bindings |

### 2.3 Trust boundaries

| Boundary | Crossed by | Control |
|---|---|---|
| Internet → Gateway | I-01, I-02, I-03 | OIDC session, CSRF token, RBAC, rate limits, security headers |
| Gateway → Core | I-07 | Shared-secret header, fail-closed; Core has no public route |
| Core → monitored endpoints | I-05 | Outbound only; no inbound path is opened |
| Core → notification destinations | I-06 | Outbound only; endpoint scheme validated at configuration time |
| Gateway → IdP | I-04 | TLS; ID token signature verified against provider JWKS |

**The single most important external property**: Core is not
addressable from the internet. It has no public route and no
platform-generated development subdomain. Every request it serves
arrives through the Service Binding and carries the shared secret.

---

## 3. External interface catalogue

| ID | Interface | Direction | Consumer | Section |
|---|---|---|---|---|
| I-01 | Web user interface | inbound | Operator's browser | §4 |
| I-02 | Public HTTP API | inbound | Operator's browser | §5 |
| I-03 | Health endpoint | inbound | External monitoring | §5.6 |
| I-04 | OIDC client | outbound | Identity provider | §6.1 |
| I-05 | Probe egress | outbound | Monitored endpoints | §6.2 |
| I-06 | Notification egress | outbound | Webhook / Slack / SMTP | §6.3 |
| I-07 | Service Binding | internal | Gateway → Core | §7 |
| I-08 | CSV export | outbound (download) | Spreadsheet software | §8.1 |
| I-09 | Configuration document | bidirectional | Operator, other deployments | §8.2 |
| I-10 | Configuration surface | inbound | Deployment operator | §9 |
| I-11 | Scheduler trigger | inbound | Platform | §10 |

---

## 4. I-01 — Web user interface

### 4.1 Navigation model

Navigation is grouped by **operator intent**, not by resource type. The
three groups correspond to the three things an operator does.

| Group | Meaning | Screens | Visible to |
|---|---|---|---|
| **Observe** | See what is happening | Dashboard, Incidents, Stats | any |
| **Operate** | Change what is monitored | Targets, Channels, Maintenance | any |
| **Verify** | Prove what happened | Audit, Settings, Migration | admin only |

The Verify group is **not rendered at all** for a member — not
disabled, not hidden by stylesheet. A member's page source contains no
trace of it.

Personal controls sit outside the three groups, in a user chip at the
top right:

- Account and session self-service (`/me/security`)
- Sign out (`/auth/logout`)

The separation is deliberate: workspace navigation answers "what am I
working on", the user chip answers "who am I and how do I leave".

### 4.2 Screen catalogue

| ID | Route | Screen | Role | Primary responsibility |
|---|---|---|---|---|
| S-01 | `/` | Dashboard | any | Is anything wrong, and what needs action now |
| S-02 | `/targets` | Target list | any | Locate a monitored endpoint |
| S-03 | `/targets/:id` | Target detail | any (owner-scoped) | Everything about one target |
| S-04 | `/incidents` | Incident queue | any | Work through unresolved failures |
| S-05 | `/maintenance` | Notification suppression | any | Schedule quiet periods |
| S-06 | `/channels` | Channel list | any | Manage notification destinations |
| S-07 | `/channels/:id` | Channel detail | any | Inspect and test one destination |
| S-08 | `/stats` | SLA overview | any | Reliability across targets |
| S-09 | `/stats/:id` | SLA detail | any (owner-scoped) | Reliability of one target |
| S-10 | `/audit` | Audit log | admin | Review recorded changes |
| S-11 | `/me/security` | My security | any | Own account, sessions, logins |
| S-12 | `/settings` | Settings | admin | User administration |
| S-13 | `/admin/migration` | Migration | admin | Export and import configuration |

Thirteen screens. Every one renders a single `<main>` landmark and a
single visible `<h1>`.

### 4.3 View state in the URL

Screens with internal sections express that state in the URL, so a view
is linkable, survives reload, and works with the browser's back button
— all without JavaScript.

| Screen | Parameter | Accepted values | Default |
|---|---|---|---|
| S-03 Target detail | `?tab=` | `overview`, `results`, `channels`, `settings` | `overview` |
| S-08 / S-09 SLA | `?window=` | `24h`, `7d`, `30d`, `90d` | `24h` |

An unrecognised value falls back to the default rather than producing an
error, so a hand-edited or truncated URL degrades gracefully.

### 4.4 Screen specifications

#### S-01 — Dashboard `/`

**Question answered:** "Is the monitored environment healthy, and what
requires action right now?"

**Content order is normative** — it encodes the priority:

1. **Metric strip** — four figures: total targets, up, down, open
   incidents. Each carries a tone derived from its own value, so a
   non-zero down count reads as alarming and a zero one does not.
2. **Open incidents** — only open ones. Each row: status, target
   (linked), cause, opened-at. Ends with a link to the full queue.
3. **Status breakdown** — unknown, disabled. (Degraded and maintenance
   target-status categories were removed — subject 17, G-28, DEC-014 —
   `decide_transition` never produces either, so both were structurally
   always zero; this line previously also said "suppressed," which was
   never an actual breakdown category the code produced.)

**Section 3 is omitted entirely when every count within it is zero.**
This is a design rule, not an optimisation: a healthy system should
produce a quiet dashboard, and a row of zeroes is noise that trains the
operator to skim.

**Scoping:** a member sees only targets they own; every figure on the
screen reflects that scope, including the counts.

**Empty state:** "All clear — no open incidents right now."

**Refresh:** manual. Automatic polling is deliberately absent — see
§13.1.

#### S-02 — Target list `/targets`

**Content:** a table of targets — status badge, name (linked), type,
host and port, interval, last check.

**Filters:** status, type, tag, and (admin only) owner.

**Behaviour:**
- A disabled target remains listed, marked with a text label and
  rendered as visually secondary. It is never silently hidden.
- Row navigation uses a real link inside the row, not a click handler
  on the row element, so keyboard and middle-click both behave.
- Admin creation controls appear as explicit buttons, never revealed
  only on hover.

#### S-03 — Target detail `/targets/:id`

**Design intent:** one target is one operational unit. Diagnosis,
history, routing, and configuration all live here rather than being
scattered across screens.

A persistent header is visible on every tab: status badge, type,
host/port/path, last check time.

| Tab | `?tab=` | Content |
|---|---|---|
| Overview | `overview` | Decision criteria (expected status, body substring, TLS threshold), consecutive counters, and protocol-specific explanatory text |
| Recent results | `results` | Most recent check results with pass/fail marking |
| Notifications | `channels` | Attached channels; attach/detach controls for admins |
| Settings | `settings` | Timeout, retries, interval, owner, tags |

**Protocol-specific help text** is shown on Overview and states exactly
what "normal" means for that target's type — the HTTP explanation
differs from the TCP one, which differs from the TLS one. An
unrecognised type omits the help block rather than showing a generic
placeholder.

**Load behaviour:** only the active tab's data is requested. Switching
tabs is a navigation, not a client-side toggle.

#### S-04 — Incident queue `/incidents`

**Structure:** open incidents first, in their own region and announced
as a status region; resolved incidents below, collapsed behind a
disclosure control.

**Row content:** status, target (linked), cause, opened-at, duration;
resolved rows additionally show the resolution note.

**Manual resolution (admin):** opens a dialog requiring a structured
reason:

| Code | Label |
|---|---|
| `recovered_externally` | Recovered externally |
| `transient` | Transient — already healthy |
| `target_removed` | Target was removed |
| `other` | Other (specify below) |

Free text is mandatory when `other` is chosen. The submitted note takes
the form `[code] free text`, which keeps the API unchanged while making
the reason machine-aggregatable.

**Required copy:** the dialog states that manual resolution changes
neither the check policy nor the target's actual health. Without this,
operators read "resolve" as "fix".

#### S-05 — Notification suppression `/maintenance`

**Terminology is load-bearing.** The screen is titled *notification
suppression*, never *maintenance*, because operators consistently read
"maintenance window" as "monitoring stops".

**Situation control (DEC-013).** A window states two behaviours
independently — whether it silences notifications and whether it
excludes its time from SLA — but the form does not expose them as two
raw checkboxes. It offers three named situations, links or radios (no
script; NFR-A11Y-10), each stating its own consequence:

| Situation | Silences notifications | Excludes from SLA | |
|---|---|---|---|
| Planned maintenance | yes | yes | default |
| Known external outage | yes | no | downtime was real; not forgiven |
| Expected noise | no | yes | keep alerting; not counted |

A help card states the semantics that always hold, plus the two that
now depend on the chosen situation:

1. Checks continue to run.
2. Incidents are still recorded.
3. Notifications are suppressed — *when this window silences*.
4. SLA calculation excludes the window — *when this window excludes*.

**Listings** show both behaviours per window as text, not colour alone,
so an operator scanning the list does not have to open a window to
learn whether it is moving the SLA figure.

**Sections:** active windows (marked as such), then upcoming and past
windows.

**Times:** displayed with an explicit UTC label and emitted in a
machine-readable element carrying the exact instant, so a reader in
another zone is never guessing.

#### S-06 / S-07 — Channels `/channels`, `/channels/:id`

**List content:** enabled state, name, type, endpoint summary, attached
target count, last test result.

**Detail content:** endpoint, attached targets, edit form, and a
separated destructive region.

**Test send:** dispatches through the real notification path, not a
simulation. The result appears **inline on the page** in a live region.
Browser dialogs are not used anywhere in the product.

**Rate-limit presentation:** a rate-limited test does not surface the
raw retry header. `Retry-After: 90` is rendered as *"Try again in about
1.5 minutes."* — the largest meaningful unit, in a sentence.

**Attached targets are shown on the detail screen** specifically so that
deleting a channel is not a blind action.

#### S-08 / S-09 — SLA `/stats`, `/stats/:id`

**Window selector:** a tab control backed by links, reflected in
`?window=`. It works with scripting disabled and produces bookmarkable
URLs.

**Columns:** target, gross uptime, SLA uptime, downtime, suppressed
time, incident count, mean time to recovery.

**Required explanation:** the difference between gross and SLA uptime
is explained adjacent to the figures. Two similar percentages side by
side with no explanation is a misreading waiting to happen.

**Export:** an aggregate CSV plus a per-row CSV control (I-08). Export
respects the same role scoping as the screen.

#### S-10 — Audit log `/audit`

**Columns:** time, actor, action, resource, result, source address,
changes.

**Change inspection:** before/after values expand in place behind a
disclosure control. Rows with neither value show a dash and no control,
rather than an empty expander.

**Unknown action types** are displayed verbatim rather than being
mapped to "other" — discarding information in an audit view defeats
its purpose.

**Integrity check** is not on this screen; it is on S-11, because it is
an account-security action rather than a browsing action.

#### S-11 — My security `/me/security`

Available to every authenticated user, for their own account only.

| Region | Content |
|---|---|
| Account | Email, display name, role |
| Current session | Issued at, expires at, CSRF protection state, sign-out link |
| Other sessions | Other active sessions, with a revoke-all-others control |
| Recent logins | The user's own recent login records |
| Audit integrity | **admin only** — runs the chain verification and reports the classification (below) |

**Chain verification classification.** Every audit row is reported in
exactly one of four classes. The check follows the chain's own
`prev_hash → row_hash` links from genesis rather than sorting rows into
an order (subject 05, DEC-020), so a row's class never depends on how
rows happen to sort:

| Class | Meaning | Operator reading |
|---|---|---|
| **verified** | Reached by following the chain from genesis, and its content re-hashes to its stored `row_hash` | Intact |
| **legacy** | Written before the hash chain existed; both hash columns are null | Expected on databases predating 0.27.2. Not a fault |
| **tampered** | Reached, but its content does not re-hash to its stored `row_hash` | **The row was altered after it was written** |
| **orphaned** | Carries hashes but is not reachable from genesis — the link that should reach it is missing or points elsewhere | **A row before it was deleted, or the chain was forked.** The orphan itself may be untouched |

**`orphaned` is reported separately from `tampered` and must not be
collapsed into it.** They have different causes and different operator
responses: a tampered row was edited, whereas an orphan is usually
*evidence that some other row was removed*. Reporting both as "tampered"
would name the wrong row as the damaged one.

A **fork** — two rows carrying the same `prev_hash` — leaves one branch
unreachable and is therefore reported as orphans, with the count
non-zero. Under the single-writer constraint (DEC-004) a fork should be
impossible; observing one is a signal in its own right.

A **cycle** — the chain looping back on a row it already passed — is
reported **separately from the four classes**, as the identifier of the
row where the loop closes. It is a property of the chain's structure, not
of a row: the row itself is already reported in whichever of the four
classes it belongs to, and counting it twice would inflate the tampered
figure with a duplicate. A cycle cannot arise from a chain that was
written honestly — `row_hash` is a hash of content that includes
`prev_hash`, so closing a loop would require a preimage — but nothing
stops a row being written directly with an arbitrary `row_hash` value,
which is precisely the condition this check exists to report.

**An all-clear requires all four classes clean *and* no cycle.** A
verification that reports "no tampering" while a cycle or an orphan is
present is a false all-clear, which is the failure mode this whole
mechanism exists to avoid.

Results of both actions render inline in live regions.

#### S-12 — Settings `/settings`

**User administration (admin):** a table of users with an upsert form
keyed on email address, setting display name, role, and active flag.

**Deletion is not offered.** The screen states why: the audit trail
references the account, so accounts are deactivated rather than
removed. A deactivated user cannot sign in but remains resolvable in
historical audit rows.

**System information** is presented read-only at the foot of the page.

#### S-13 — Migration `/admin/migration`

| Region | Content |
|---|---|
| Export | Include-users checkbox (default off), download control |
| Import | Payload input, conflict policy (default skip), apply flag (default off), run control |
| Bulk data | Pointer to platform tooling for monitoring history, which configuration export deliberately excludes |

**Dry run is the default.** Applying changes requires explicitly setting
the apply flag and confirming. Validation reports **all** errors in one
pass rather than stopping at the first — an operator fixing an import
should need one round trip, not ten.

### 4.5 Common component semantics

| Component | Markup contract | Accessibility contract |
|---|---|---|
| Skip link | First focusable element on every page | Targets the main landmark |
| Navigation | Labelled group per verb | Active item marked programmatically |
| Status badge | Shape marker plus text label plus colour | Colour is never the sole signal |
| Metric card | Region with label, value, optional hint | Tone derives from value |
| Tabs | Links, not scripted toggles | Active tab marked as current page |
| Inline result | Live region | Announced without stealing focus |
| Unrecorded-operation warning | Rendered in the operation's existing inline result region, **alongside** the success message, never replacing it | Announced in the live region; carries a text marker, not colour alone |
| Timestamp | Machine-readable element carrying the exact instant | Unambiguous across locales |
| Destructive region | Separated region with confirmation | Focus returns on cancel |

**Required copy for the unrecorded-operation warning.** State that the
change took effect *before* stating that the record failed — in that
order, because the operator's first question is "did it happen?".
Suggested: *"Change applied. It could not be written to the audit log —
please record it manually."* Calm and factual, per the terminology
rules: no alarm decoration.

### 4.6 Responsive behaviour

| Viewport | Navigation | Tables | Supporting content |
|---|---|---|---|
| Desktop | Persistent rail | Full columns | Side region |
| Tablet | Rail or compact bar | Priority columns; rest collapsible | Below summary |
| Mobile | Bottom navigation, key routes | Card representation | Inline below content |

On narrow viewports, primary form actions are pinned to the bottom of
the viewport so they remain reachable without scrolling past a long
form.

### 4.7 Accessibility contract

These properties hold on **every** screen, and are verified
mechanically rather than by inspection:

- Contrast meets WCAG 2.1 AA; 25 colour pairs are pinned across light
  and dark themes by an automated check that fails the build on
  regression.
- Status is never conveyed by colour alone.
- All functionality is keyboard-operable using native element
  behaviour; there is no scripted focus management to go wrong.
- Landmarks are present on every page; exactly one main region.
- Motion is suppressed when the user has requested reduced motion.
- Pages remain readable without stylesheets and operable without
  scripting.
- Browser dialog primitives are not used. There are zero occurrences in
  the product.

---

## 5. I-02 — Public HTTP API

### 5.1 Conventions

| Aspect | Contract |
|---|---|
| Transport | HTTPS only |
| Authentication | Session cookie established by the OIDC flow |
| Authorization | Role checked server-side on every request, independent of UI visibility |
| CSRF | Required on every state-changing endpoint |
| CSRF transport | Token surfaced to the page as a meta element; submitted in the `X-CSRF-Token` request header |
| Request body | JSON for API endpoints; form encoding for HTML form posts |
| Response body | JSON for API endpoints; HTML for screens; CSV for exports |
| Character encoding | UTF-8 throughout |
| **Unrecorded mutation** | A state-changing endpoint that completes successfully but whose audit record could not be written returns **200 with the `X-Audit-Warning: 1` response header**. The operation *has* taken effect; the header states that it was not recorded. The header is **absent** — not `0` — when the audit row was written |

**Why an unrecorded mutation is not an error status.** The mutation
succeeded. Returning 500 would tell the operator the opposite of what
happened, and there is no transaction spanning the business write and
the audit write to make a rollback possible. The honest report is
"done, but not recorded" — which is actionable, where a false failure
is not.

**Why a header rather than a body field.** Every state-changing endpoint
would otherwise need its response body reshaped, and several return a
bare string. A header attaches uniformly to any response, is invisible to
clients that do not look for it, and cannot collide with an existing
body schema. **An API client that ignores it sees exactly today's
behaviour** — which is intended: the warning is an operator-facing signal,
not a contract change that breaks integrations.

*Header name added 2026-08-02 with subject 07. The contract above —
200 plus a warning indicator — predates it (DEC-011); only the
indicator's form was unspecified, and code should not have been the
first place it was decided (§14).*

### 5.2 Status code taxonomy

| Code | Meaning in this system |
|---|---|
| 200 | Success |
| 302 | Redirect after authentication or after a form post |
| 400 | Malformed request or failed field validation |
| 401 | No valid session |
| 403 | Authenticated but not permitted — also returned for an unregistered or deactivated subject |
| 404 | Resource does not exist, **or** exists but is outside the caller's ownership scope |
| 429 | Rate limit exceeded; accompanied by retry timing |
| 500 | Unexpected server-side failure |

**403 versus 404 is a deliberate choice.** A member requesting a target
owned by someone else receives 404, not 403, because 403 would confirm
the resource exists. Ownership scope is treated as visibility, not as
permission.

### 5.3 Authentication endpoints

| Method | Path | Role | CSRF | Purpose |
|---|---|---|---|---|
| GET | `/auth/login` | none | — | Begin the OIDC authorization flow |
| GET | `/auth/callback` | none | — | Complete the flow; establish a session |
| GET | `/auth/logout` | any | exempt | Sign out via a plain link |
| POST | `/auth/logout` | any | required | Sign out via form submission |

**The GET logout exemption is intentional and bounded.** Sign-out must
be reachable as an ordinary link for the interface to work without
scripting; the POST variant enforces the token normally. The exemption
is recorded rather than incidental.

### 5.4 State-changing endpoints

All require a valid session and a valid CSRF token.

| Method | Path | Role | Purpose |
|---|---|---|---|
| POST | `/api/targets` | admin | Create a target |
| PUT | `/api/targets/:id` | admin | Update a target |
| DELETE | `/api/targets/:id` | admin | Delete a target and its dependents |
| POST | `/api/targets/:id/channels` | admin | Attach a channel to a target |
| DELETE | `/api/targets/:id/channels/:channel_id` | admin | Detach a channel |
| POST | `/api/incidents/:id/resolve` | admin | Resolve an incident with a structured reason |
| POST | `/api/maintenance` | admin | Create a suppression window |
| POST | `/api/channels` | admin | Create a channel |
| PUT | `/api/channels/:id` | admin | Update a channel |
| DELETE | `/api/channels/:id` | admin | Delete a channel |
| POST | `/api/channels/:id/test` | admin | Send a test notification |
| POST | `/api/settings/users` | admin | Create or update a user |
| POST | `/api/admin/migration/import` | admin | Validate or apply a configuration import |
| POST | `/api/me/sessions/revoke-others` | any | Revoke the caller's other sessions |

Fourteen endpoints. Every one is CSRF-protected; there are no
exceptions in this table.

### 5.5 Read endpoints

| Method | Path | Role | Returns |
|---|---|---|---|
| GET | `/api/targets/:id/results` | any (owner-scoped) | Recent check results |
| GET | `/api/stats/sla` | any (owner-scoped) | SLA report as JSON |
| GET | `/api/stats/sla.csv` | any (owner-scoped) | SLA report as CSV |
| GET | `/api/stats/incidents/:id.csv` | any (owner-scoped) | Incident history for one target as CSV |
| GET | `/api/admin/audit/verify` | admin | Audit chain verification result |
| GET | `/api/admin/migration/export` | admin | Configuration document |

**Six** read endpoints. Earlier documentation states seven; that figure
was incorrect (§13.2).

Query parameters:

| Endpoint | Parameter | Values |
|---|---|---|
| `/api/stats/sla`, `/api/stats/sla.csv` | `window` | `24h`, `7d`, `30d`, `90d` |
| `/api/stats/incidents/:id.csv` | `window` | as above |
| `/api/admin/migration/export` | `include_users` | boolean |

### 5.6 I-03 — Health endpoint

| Method | Path | Role | Response |
|---|---|---|---|
| GET | `/healthz` | none | 200 with a fixed status body |

Three properties are contractual:

1. **No authentication.** It exists to be polled by an external checker.
2. **No dependency on Core.** The Gateway answers alone, so the endpoint
   stays available when the private component or the database is
   degraded. It reports "the public surface is serving", not "the whole
   system is healthy" — a distinction the operations documentation
   states explicitly.
3. **No information disclosure.** The body is fixed; it reveals no
   version, configuration, or internal state.

### 5.7 Rate limiting

| Endpoint | Limit | Key |
|---|---|---|
| `/auth/login` | 10 per minute, 50 per hour | Client address |
| `/api/channels/:id/test` | 15 per minute | Client address |

A limited request receives 429 with retry timing. The interface renders
that timing as prose (§4.4, S-06) rather than exposing the raw header
value to the operator.

---

## 6. Outbound interfaces

### 6.1 I-04 — Identity provider

**Flow:** OpenID Connect Authorization Code with PKCE, plus `state` and
`nonce`.

| Step | Direction | Content |
|---|---|---|
| Discovery | outbound | `GET {issuer}/.well-known/openid-configuration` |
| Authorization | redirect | Client identifier, redirect URI, scopes, `state`, `nonce`, PKCE challenge |
| Callback | inbound | Authorization code and `state`, or an error with description |
| Token exchange | outbound | Code plus PKCE verifier, authenticated with the client secret |
| Key retrieval | outbound | JWKS document, cached |

**Endpoint resolution is discovery-only.** The issuer URL is
configuration; the authorization, token, and JWKS endpoints are read
from the discovery document. There are **no per-endpoint override
variables**, which means a provider that does not publish a discovery
document is not currently supported (§13.2).

**Validation performed before any claim is trusted:** signature against
the provider's JWKS, issuer match, audience match, expiry, and `nonce`
match. A `state` mismatch aborts the flow before token exchange.

**Identity mapping:** the subject claim maps to a user record. An
unmapped subject, or one mapped to a deactivated user, is refused with
403 — there is no self-registration path.

### 6.2 I-05 — Probe egress

What Noye sends to the systems it monitors. All probes are outbound
only; monitoring opens no inbound path.

| Type | Request | Success condition |
|---|---|---|
| `http` / `https` | `GET` to the composed URL, honouring the configured timeout | Connection established, no timeout, status matches expectation (default 200), and — when configured — the response body contains the expected substring |
| `tcp` | Connection attempt to host and port | Connection established within the timeout |
| `smtp` | Connection to host and port (25, 465, or 587) | Server returns a `220` greeting |
| `tls` | Handshake against the endpoint | Handshake succeeds and remaining certificate validity is at least the configured threshold (default 30 days) |

**Retry semantics are externally visible in one respect**: retries occur
*within a single sweep* and do not each count as an independent
failure. A monitored endpoint may therefore see up to `retry_count + 1`
connection attempts in quick succession, and should not interpret that
as multiple failed checks.

**Politeness contract:** one target is probed at most once per its
configured interval, regardless of how many sweeps occur.

### 6.3 I-06 — Notification egress

Dispatched **only on a state transition** — never on every check. A
target that has been failing for six hours produces one notification,
not three hundred and sixty.

#### 6.3.1 Message content

Two message shapes are produced.

**Down:**

```
title: [DOWN] {target name} is unreachable
body:  Target {name} ({host}) is down.
       Error: {error message, or "Unknown"}
       Response time: {n}ms
```

**Recovery** follows the same structure with the recovery wording.

Every message carries: title, body, status (`down`, `up`, or `test`),
target name, target host, and an RFC 3339 UTC timestamp.

#### 6.3.2 Webhook payload

`POST` with `Content-Type: application/json`:

```json
{
  "title":       "[DOWN] web-01 is unreachable",
  "body":        "Target web-01 (10.0.0.1) is down.\nError: …\nResponse time: 5000ms",
  "status":      "down",
  "target_name": "web-01",
  "target_host": "10.0.0.1",
  "timestamp":   "2026-05-04T12:34:56Z"
}
```

This is a **stable external contract**. Receivers parse these six
fields; changing or removing one is a breaking change to every
integration.

#### 6.3.3 Slack payload

Slack receives a **Block Kit document**, not the generic payload:

```json
{
  "attachments": [{
    "color": "#dc3545",
    "blocks": [
      { "type": "section",
        "text": { "type": "mrkdwn",
                  "text": ":red_circle: *{title}*\n{body}" } },
      { "type": "context", "elements": [ … ] }
    ]
  }]
}
```

| Status | Colour | Emoji |
|---|---|---|
| `down` | `#dc3545` | `:red_circle:` |
| `up` | `#28a745` | `:large_green_circle:` |
| `test` | `#6c757d` | `:wrench:` |

**This corrects a documented claim.** The roadmap and the associated
proposal describe Slack as receiving "the same generic JSON as
webhook". It does not, and has not for some time (§13.2). The open
proposal is an *enrichment* — header block, structured fields, and a
deep link back into the interface — not an introduction of Slack
formatting.

#### 6.3.4 Email

| Property | Contract |
|---|---|
| Transport | Implicit TLS on port 465; STARTTLS otherwise; overridable by configuration |
| Authentication | Strongest advertised, preferring SCRAM-SHA-256, then PLAIN, then LOGIN |
| Format | RFC 5322 / 2047 / MIME conformant |
| Message identifier | Generated per message, with the sender's domain, so relay anti-spoofing does not reject it |
| Recipients | One channel corresponds to one recipient; fan-out by blind copy is not performed |
| Body | Plain text |

**Graceful degradation:** when SMTP configuration is absent, email
channels are skipped with a log entry rather than failing. When
configuration is present but invalid, the send reports an error. The
distinction matters operationally: "not configured" and "misconfigured"
are different problems and are surfaced differently.

**Isolation guarantee:** a notification failure of any kind never
prevents monitoring, state updates, or incident recording.

---

## 7. I-07 — Service Binding (Gateway → Core)

An internal interface, specified here because it is a contract between
independently deployed components.

### 7.1 Call contract

| Header | Content | Enforcement |
|---|---|---|
| `X-Gateway-Token` | Shared secret | Verified fail-closed; absent or wrong is rejected |
| `X-Caller-Id` | Authenticated user identifier | Propagated for authorization and audit attribution |
| `X-Caller-Email` | Authenticated user email | Propagated |
| `X-Caller-Role` | `admin` or `member` | Propagated |

Core does not resolve sessions. Identity arrives already established;
this keeps session handling in exactly one component.

### 7.2 Internal endpoint surface

Thirty-seven endpoints, grouped by resource:

| Group | Endpoints |
|---|---|
| Health | `GET /healthz` |
| Targets | list, summary, states, create, read, update, delete, results, state, incidents, channels (attach/detach), SLA, multi-window SLA |
| Channels | list, create, read, update, delete, test, targets-for-channel |
| Incidents | list, resolve |
| Maintenance | list, create |
| Stats | SLA report |
| Audit | list, verify, login history, record login |
| Users | list, create/update, lookup by email |
| Migration | export, import |

The Gateway exposes a deliberately narrower surface than Core provides;
not every internal endpoint has a public counterpart.

---

## 8. Data exchange formats

### 8.1 I-08 — CSV export

**Encoding contract:** RFC 4180 — CRLF line endings, quoted fields
where required, doubled quotes for escaping — preceded by a UTF-8 byte
order mark. Content type is `text/csv; charset=utf-8`.

**The byte order mark is required, not incidental.** Without it,
spreadsheet software defaulting to a local codepage renders non-ASCII
target names as mojibake. The declared charset alone does not prevent
this.

**SLA summary** (`/api/stats/sla.csv`) — eleven columns, in order:

```
target_id, target_name, window_start, window_end, window_seconds,
gross_uptime_percent, sla_uptime_percent, downtime_seconds,
excluded_seconds, incident_count, mttr_seconds
```

`mttr_seconds` is empty when no incident resolved within the window.
`sla_uptime_percent` is empty, not `100`, when the entire window was
excluded (FR-SLA-09).

**Breaking change (DEC-013, subject 13).** Column 9 was
`maintenance_seconds`, now `excluded_seconds`. Under the suppression/SLA
split (§13 D-2) that quantity is "time excluded from SLA", which is no
longer the same fact as "time inside a maintenance window" — a window can
now silence without excluding, or exclude without silencing. Per §14 this
carries a `CHANGELOG.md` entry and a migration note for anyone parsing
the export.

**Incident history** (`/api/stats/incidents/:id.csv`) — ten columns:

```
incident_id, target_id, status, opened_at, resolved_at,
duration_seconds, cause, resolution_note, opened_by, resolved_by
```

**Breaking change (DEC-013's own gap register entry, G-29, subject 16).**
Column 9 was `created_by`, which meant "who opened it" for open rows and
"who resolved it" for resolved ones — a consumer parsing the export could
not tell which. Split into `opened_by` (column 9) and `resolved_by`
(column 10, empty for open incidents). This is the second breaking
change to this interface in the same unreleased version, alongside the
SLA export's `maintenance_seconds` → `excluded_seconds` rename (subject
13) — both are published as one coherent breaking-change section in
`CHANGELOG.md`, not two entries a reader has to reconcile.

Both exports honour the caller's ownership scope: a member's export
contains only their own targets.

### 8.2 I-09 — Configuration document

A self-describing, versioned document used for backup, environment
replication, and migration between deployments.

**Envelope:**

| Field | Content |
|---|---|
| `schema_version` | Integer; consumers check compatibility |
| `exported_at` | Export instant |
| `source_deployment` | Optional human-readable label identifying the origin |
| `data` | The exported collections |

**Included collections:** targets, channels, target-to-channel
attachments, suppression windows, and — at the operator's option —
users.

**Deliberately excluded:**

| Excluded | Reason |
|---|---|
| Secrets of any kind | A configuration document is not a credential store |
| Audit history | Tamper-evidence does not survive export and re-import; audit history moves by database export |
| Check results and incidents | Operational history, not configuration; moved by platform tooling |

**Import contract:**

| Parameter | Values | Default |
|---|---|---|
| `payload` | A document of the shape above | — |
| `on_conflict` | skip / replace / fail | skip |
| `apply` | boolean | **false** |

**Dry run is the default.** With `apply` false, the server validates
and reports the counts that *would* be written, changing nothing.
Validation collects all errors rather than stopping at the first.

**Cross-reference validation.** Every reference the document carries —
notably the owner of each target and channel — is resolved against the
receiving deployment **before any write**. Unresolvable references are
reported together, with counts, and are never silently remapped to the
importing operator. Because `include_users` defaults to off, a document
exported with default options will not import into a deployment that
does not already contain the referenced users; the validation message
states that, and what to do about it.

**`replace` updates in place.** The conflict policy replaces an
object's *configuration*; it does not replace the object. Monitoring
history, incidents and channel attachments belonging to an updated
target are preserved.

**Provenance.** An imported object records the importing operator as its
creator and last updater — the values in the document identify
principals of the deployment that produced it and do not resolve here.
The document's origin is preserved through the envelope's
`source_deployment` field and the audit row written for the import.

Applied imports are recorded in the audit trail.

---

## 9. I-10 — Configuration surface

What a deployment operator sets. This is an external interface: the
names are a contract, and changing one breaks existing deployments.

### 9.1 Gateway

| Key | Kind | Required | Purpose |
|---|---|---|---|
| `OIDC_ISSUER_URL` | variable | yes | Issuer; discovery is derived from it |
| `OIDC_CLIENT_ID` | variable | yes | Client identifier |
| `OIDC_REDIRECT_URI` | variable | yes | Callback URL |
| `OIDC_SCOPES` | variable | no | Requested scopes |
| `OIDC_CLIENT_SECRET` | **secret** | yes | Token exchange |
| `GATEWAY_SHARED_TOKEN` | **secret** | yes | Service Binding authentication; must match Core |
| `SESSION_COOKIE_NAME` | variable | no | Cookie name override |
| `SESSION_DURATION_MIN` | variable | no | Session lifetime in minutes |
| `NOYE_ENV` | variable | no | `production` (default) or `development` |
| `DEPLOYMENT_LABEL` | variable | no | Label carried into configuration exports |
| `TURNSTILE_SITE_KEY` | variable | no | Reserved; challenge not yet activated |
| `TURNSTILE_SECRET_KEY` | **secret** | no | Reserved; challenge not yet activated |

### 9.2 Core

| Key | Kind | Required | Purpose |
|---|---|---|---|
| `GATEWAY_SHARED_TOKEN` | **secret** | yes | Must match the Gateway value |
| `EMAIL_SMTP_HOST` | variable | no | Presence enables email delivery |
| `EMAIL_SMTP_PORT` | variable | with email | Relay port |
| `EMAIL_SMTP_USERNAME` | variable | with email | Relay account |
| `EMAIL_SMTP_PASSWORD` | **secret** | with email | Relay credential |
| `EMAIL_SMTP_TLS_MODE` | variable | no | Overrides port-derived TLS mode |
| `EMAIL_FROM_ADDRESS` | variable | with email | Sender address; determines the message-identifier domain |
| `EMAIL_FROM_NAME` | variable | no | Sender display name |

### 9.3 Configuration behaviour contracts

| Contract | Behaviour |
|---|---|
| **Configuration source** | `crates/*/wrangler.toml.example` is tracked; the operator copies it to `wrangler.toml`, which is **not** tracked. No file in the repository is deployable as-is (NFR-SEC-14) |
| **Declared environment** | The template declares `production`. The Gateway template declares `workers_dev = false` |
| Unset environment | Treated as production — the restrictive setting, not the permissive one |
| **Published-credential rejection** | Any credential value that has appeared in the repository is refused at request time in **every** environment, unconditionally — including `development`. The refusal MUST NOT be conditioned on a variable the shipped configuration sets (NFR-SEC-15) |
| Email activation | Governed solely by the presence of SMTP host configuration |
| Secret handling | Secrets are supplied through the platform's secret mechanism; they never appear in the repository, the release archive, or any export |

*Amended 2026-07-28 (v0.28.0). Key **names** are unchanged, so no
deployed environment breaks; only how they are supplied changes.*

### 9.5 Local development configuration

Local development supplies `GATEWAY_SHARED_TOKEN` and
`OIDC_CLIENT_SECRET` through `.dev.vars`, which is not tracked. The
values must be generated locally: the values that previously shipped in
the repository are permanently refused, in every environment, because a
value published once stays published. Both Workers must carry the same
`GATEWAY_SHARED_TOKEN`.

### 9.4 Storage bindings

| Binding | Store | Contains |
|---|---|---|
| Database | D1 | System of record: targets, states, results, incidents, windows, channels, attachments, audit, users, retention policy |
| Cache | KV | Sessions, transient OIDC state, key cache, rate-limit counters |
| Bucket | R2 | Archived history and export artifacts |

**Externally visible consequence:** losing the KV store signs everyone
out and loses nothing else. This is a designed property, not an
accident of layering.

---

## 10. I-11 — Scheduler trigger

| Property | Contract |
|---|---|
| Registrations | Exactly one |
| Interval | One minute |
| Selection | Enabled targets whose next check time has passed |
| Execution | Due targets probed concurrently within platform limits |

**One trigger, not many.** The original requirement is explicit on this
point: a single scheduler processes everything that has come due,
rather than registering a trigger per interval. Interval variation is a
property of target data, not of the trigger.

**Externally observable timing:** a target configured with a five
minute interval is checked on the first sweep at or after each five
minute boundary. Checks are not aligned to wall-clock boundaries, and
no guarantee of exact spacing is made.

---

## 11. Externally observable security surface

### 11.1 Response headers

Applied to HTML responses:

| Header | Purpose |
|---|---|
| Content Security Policy | Restricts sources to same-origin |
| Strict Transport Security | Production only; six-month max-age including subdomains |
| Content type options | Prevents type sniffing |
| Frame options | Denies framing |
| Referrer policy | Restricts referrer leakage cross-origin |
| Permissions policy | Denies geolocation, microphone, and camera |

Export responses retain their declared content type so that browsers do
not reinterpret them.

### 11.2 Session cookie

| Attribute | Value | Reason |
|---|---|---|
| `HttpOnly` | always | Not readable by script |
| `SameSite` | `Lax` | `Strict` would drop the cookie on return from the identity provider |
| `Secure` | outside development | Production is HTTPS-only |
| `Path` | `/` | Applies across the interface |
| Lifetime | bounded, default 8 hours | Configurable |

The resulting CSRF exposure from `Lax` is closed by the token
mechanism, not by cookie attributes alone.

### 11.3 Redirect policy

Post-authentication redirects accept **same-origin absolute paths
only**. Scheme-relative, protocol-relative, and cross-host destinations
fall back to the root. Header-injection attempts embedded in a redirect
target are rejected.

### 11.4 What is deliberately not exposed

| Not exposed | Reason |
|---|---|
| Version or build information on the health endpoint | Fingerprinting |
| Existence of resources outside the caller's scope | 404 rather than 403 (§5.2) |
| Internal role labels in user-facing pages | Information leakage |
| Secrets in any export | Configuration documents are not credential stores |
| Stack traces or internal identifiers in error responses | Error bodies are operator-facing, not developer-facing |

---

## 12. Traceability

| Interface | Implements |
|---|---|
| I-01 Web UI | FR-UI-01…20, NFR-A11Y-01…13, FR-RBAC-04…05 |
| I-02 HTTP API | FR-TGT-06, FR-INC-04, FR-NTF-01…04, FR-MIG-05…07, FR-CSRF-01…05, FR-RBAC-06 |
| I-03 Health | FR-OPS-01…03 |
| I-04 OIDC | FR-AUTH-01…04, FR-RBAC-02…03 |
| I-05 Probes | FR-CHK-01…10, FR-MON-04 |
| I-06 Notifications | FR-NTF-07…14, FR-MON-08…09, FR-SUP-06 |
| I-07 Service Binding | NFR-SEC-01…03 |
| I-08 CSV | FR-SLA-06…08 |
| I-09 Configuration document | FR-MIG-01…09 |
| I-10 Configuration surface | NFR-SEC-07…09, CON-03…06 |
| I-11 Scheduler | FR-MON-01…03, CON-05 |
| §11 Security surface | NFR-SEC-04…06 |

---

## 13. Design notes and divergences

### 13.1 Deliberate absences

| Absent | Reason |
|---|---|
| Automatic dashboard refresh | Polling that respects reduced-motion preferences and does not disturb keyboard focus costs more than manual reload is worth at this scale |
| Client-side framework | Conflicts with the requirement that the interface work without scripting |
| Modal-heavy workflows | Dedicated pages and inline panels are preferred; dialogs are reserved for destructive or irreversible actions |
| Hover-only controls | Unreachable by keyboard and touch |
| Toast-only feedback | Transient and easily missed by assistive technology |

### 13.2 Corrections to earlier documents

Verifying the interfaces against source turned up four claims in
existing documentation that are wrong. They are listed here so the
error does not propagate further.

| Claim | Where it appears | Reality |
|---|---|---|
| Slack receives the same generic JSON as webhook | Roadmap; Slack payload proposal; handoff bundle; `requirements.md` FR-NTF-12 | **Slack already receives a Block Kit document** with per-status colour, emoji, a mrkdwn section, and a context block. The open proposal is enrichment, not introduction. FR-NTF-12 should read `Partial`, not `Deferred` |
| Per-endpoint OIDC overrides are available | `requirements.md` FR-AUTH-03 | **Not implemented.** Endpoint resolution is discovery-only; there are no override variables. A provider without a discovery document is unsupported |
| Seven read API endpoints | Development instruction v2; status summaries | **Six.** The count appears to have included the health endpoint |
| Variable names `OIDC_ISSUER`, `SESSION_DURATION_MINUTES` | `requirements.md`, handoff bundle | Actual names are `OIDC_ISSUER_URL` and `SESSION_DURATION_MIN`. Two further variables, `OIDC_SCOPES` and `DEPLOYMENT_LABEL`, were undocumented |

### 13.3 External-design consequences of open requirement gaps

Several gaps recorded in `requirements.md` §11 are visible at the
interface, not merely internal:

| Gap | External consequence |
|---|---|
| G-07 suppression flag ignored | A window configured as non-suppressing still silences notifications. The interface offers a control that does not do what it says |
| G-09 substring tag matching | A window scoped to one tag silently applies to targets carrying a longer tag with the same prefix. Over-suppression is invisible to the operator |
| G-12 SLA denominator | The figure labelled "SLA uptime" is not computed the way the adjacent explanation describes |
| G-10 missing automatic duration | The mean-time-to-recovery column omits automatically resolved incidents — the large majority — making a displayed number misleading rather than merely incomplete |
| G-05 / G-06 import gaps | Import reports success while producing targets that are not actually monitorable |

All five are cases where **the interface makes a promise the
implementation does not keep**. That places them in external-design
scope, not only internal: the contract described in §4 and §5 is
currently not honoured for these paths.

### 13.4 Open external-design questions

| Question | Depends on |
|---|---|
| Should suppression separate "silence alerts" from "exclude from SLA" as two operator-visible controls? | Requirements decision D-2 |
| Does the interface need a language selector, and where does it live? | Requirements decision D-3 (multilingual support is specified but unimplemented and untracked) |
| Should incident acknowledgement appear as an operator action between open and resolved? | Requirements decision D-4 |
| Should a target-editing form exist in the interface, or does the API remain the only path? | Deferred roadmap item |

---

## 14. Document maintenance

- An interface change requires a change here **before** implementation,
  per the mandated design sequence.
- Payload shapes in §6.3, §8.1, and §8.2 are external contracts.
  Changing a field name or removing a field breaks existing
  integrations and requires a version increment plus a migration note.
- Configuration key names in §9 are equally contractual; renaming one
  breaks deployed environments.
- When a divergence in §13.2 is corrected in the source documents, the
  row is struck rather than deleted, so the correction remains
  traceable.
