# Decision log

Decisions future work must preserve or consciously revisit. Each carries
its rationale **and** its re-evaluation criteria, per NFR-MNT-04 — a
decision without a "revisit when" is an assumption in disguise.

Identifiers are stable and never reused. A superseded decision is marked,
not deleted.

---

## Architecture and implementation

### DEC-001 — Two-Worker split

| Field | Content |
|---|---|
| **Decision** | Gateway is public; Core is reachable only through a Service Binding |
| **Why** | Minimise the public attack surface; keep D1 and Cron off the internet |
| **Consequence** | Core must stay `workers_dev = false` with no route. Cross-Worker calls carry `X-Gateway-Token` and fail closed |
| **Re-evaluate when** | Never, while the platform offers Service Bindings |
| **Where enforced** | `docs/src/architecture.md`; `crates/core/wrangler.toml.example` |

### DEC-002 — Generic OIDC with self-managed RBAC

| Field | Content |
|---|---|
| **Decision** | Generic OIDC replaces Cloudflare Access as the identity layer |
| **Why** | Portability across identity providers; no vendor lock-in on identity |
| **Consequence** | We own session, CSRF and RBAC code. Cloudflare Access may still front the Gateway but must not be required |
| **Re-evaluate when** | Never expected. Note the current limitation: endpoint resolution is discovery-only, so a provider without a discovery document is unsupported (FR-AUTH-03, gap G-19) |
| **Where enforced** | `crates/gateway/src/auth/` |

### DEC-003 — Pure-function `String` UI

| Field | Content |
|---|---|
| **Decision** | Pages compose from pure functions returning `String`. Leptos SSR was evaluated and abandoned |
| **Why** | Host-target unit-testability without a Worker runtime; HTML-first; fewer dependencies |
| **Consequence** | No client framework. 435 host tests are possible as a result. **Any UI contributed from the Leptos-based mockup must be re-expressed, not merged** |
| **Re-evaluate when** | A rendering need appears that pure functions cannot express while keeping no-JS operability (NFR-A11Y-10) |
| **Where enforced** | `crates/gateway/src/ui/` |

### DEC-004 — SHA-256 hash-chained audit log

| Field | Content |
|---|---|
| **Decision** | Each audit row carries `prev_hash` and `row_hash` over a version-tagged canonical serialization |
| **Why** | Tamper-evidence for the system's evidentiary basis |
| **Consequence** | Single-writer constraint (A-05). Queue fan-out would require a Durable Object or another serialization point. The chain proves rows were not *altered*; it cannot prove rows are not *missing* — see FR-AUD-08 and gap G-26. It also requires the writer and the verifier to agree on a **single total row order**, including tie-breaking, or the check produces false positives (gap G-30) |
| **Re-evaluate when** | Target count approaches ~1000 and fan-out becomes necessary |
| **Where enforced** | `sql/0001_initial.sql`; `crates/core/src/db/audit/hash.rs`. *(Formerly cited `sql/0002_audit_hash_chain.sql`, retired by DEC-010.)* |

### DEC-005 — "Notification suppression", not "maintenance"

| Field | Content |
|---|---|
| **Decision** | All user-facing text says notification suppression. "Maintenance window" is a schema term only |
| **Why** | Operators consistently read "maintenance window" as "monitoring stops". It does not: checks continue, incidents are recorded, only notification is withheld |
| **Consequence** | The UI vocabulary diverges from industry norm by design |
| **Re-evaluate when** | Never. This was a repeated, observed misreading |
| **Where enforced** | `/maintenance` UI; FR-SUP-12 |

### DEC-006 — `Cargo.lock` committed, excluded from the release archive

| Field | Content |
|---|---|
| **Decision** | The lockfile is committed for `--locked` CI reproducibility and excluded from the release tarball |
| **Why** | Two consumers with different needs: CI wants pinned resolution, archive recipients want a clean source tree |
| **Consequence** | Recipients re-resolve dependencies |
| **Re-evaluate when** | **Open — §13 D-5.** The rule was applied but never ratified, and the parallel UI mockup adopted the opposite convention. Decide in the release-hardening phase |
| **Where enforced** | `package.sh`; `.github/workflows/ci.yml` |

### DEC-007 — Documentation-only patch releases

| Field | Content |
|---|---|
| **Decision** | Releases that ship only governance or specification changes take a patch bump |
| **Why** | Ship documentation without implying behaviour changed |
| **Consequence** | No minor bump unless behaviour changes |
| **Re-evaluate when** | Never |
| **Where enforced** | `CHANGELOG.md` |

---

## Product scope

### DEC-008 — Single tenant per deployment

| Field | Content |
|---|---|
| **Decision** | Noye is single-tenant. One deployment serves one tenant. Multi-tenancy within a single deployment is out of scope |
| **Why** | The premise is tens to a few hundred endpoints operated by a handful of people. A tenant column on eight entities, plus membership tables and tenant-scoping on every query, is a permanent complexity cost with no operator asking for it. Separate deployments already give stronger isolation than a tenant column would |
| **Consequence** | No tenant table, no tenant column. The schema constraints added in the hardening phase are final and need no reshaping. CON-08 moves to `Implemented`. The mockup's "Tenant administrator" vocabulary and its tenant-operation-authorization proposal are rejected |
| **Re-evaluate when** | A concrete operator requirement appears for two tenants sharing one Cloudflare account **and** unable to run two deployments. Revisit **before** further schema work, never after — retrofitting through a hardened schema costs considerably more |
| **Closes** | §13 D-1 |
| **Date** | 2026-07-28 |

### DEC-009 — Two roles: `admin` and `member`

| Field | Content |
|---|---|
| **Decision** | Exactly two roles, `admin` and `member`, with `member` scoped to targets they own. The four-role capability ladder (Viewer → Responder → Operator → SystemAdministrator) proposed by the parallel UI mockup is rejected |
| **Why** | FR-RBAC-01 already states this and it is enforced in the shipped code. The ladder deletes `member` — a read-only **owner** of specific targets — and replaces it with `Viewer`, read-only across the whole tenant. That is a different and weaker guarantee, and FR-RBAC-04's owner scoping, enforced in seven places today, has no home in the ladder |
| **Consequence** | No capability layer. On-call status and its Responder tier are rejected with it, consistent with the §2.4 non-goal "on-call rotation / paging escalation" |
| **Re-evaluate when** | A deployment needs a principal who can resolve incidents but not manage targets, **and** the split cannot be expressed by target ownership. Note that adding a third role is cheap; the ladder's real cost was the twelve-capability layer beneath it, not the role count |
| **Closes** | §13 D-6 |
| **Date** | 2026-07-28 |

### DEC-010 — Migration `0002` withdrawn; number retired

| Field | Content |
|---|---|
| **Decision** | `sql/0002_audit_hash_chain.sql` is deleted. `sql/0001_initial.sql` is not modified. The number `0002` is retired and never reused; the next migration is `0003` |
| **Why** | `0001` was amended to add `prev_hash`, `row_hash` and `idx_audit_row_hash` in the **same commit** that added `0002` — `5de978d`, the 0.27.2 release. `0002` therefore consists of two redundant `ALTER TABLE`s and an index `0001` already creates, and fails on every database provisioned from the current `0001`. Removing the columns from `0001` instead would not help: `0001` is already recorded as applied there, so `0002` would still fail |
| **Consequence** | `sql/` shows an intentional numbering gap. It must stay: renumbering is an RFC-lifecycle anti-pattern and DR-MIG-02 requires immutability. **Three database classes exist**, split by the 0.1.0 and 0.27.2 releases — see `rfcs/handoffs/01-migration-applicability.md`. Deleting `0002` clears Classes B and C; **Class A (provisioned at 0.1.0, never re-migrated) is repaired by subject 06's rebuild**, which names its columns explicitly for that reason |
| **Premise corrected 2026-07-28** | This decision originally cited an amendment "at 0.18.0" and *two* classes of database. **No 0.18.0 release exists** — tags are `0.0.1`, `0.1.0`, `0.27.2` — and there are three classes. The false version came from a comment inside `sql/0001_initial.sql:146` (`since 0,18.0`), which was treated as evidence about the file it sits in. Raised by the tester before Subject 01's baseline was written; see `.git-exclude/reviewed/011-reply-subject-01-migration-premise.md`. **G-01 is the direct consequence of a DR-MIG-02 violation** — editing a migration that had shipped under tag 0.1.0 |
| **Re-evaluate when** | Never. Retired numbers stay retired |
| **Closes** | Gap G-01 |
| **Date** | 2026-07-28 |

### DEC-011 — Audit write failure: surface and complete

| Field | Content |
|---|---|
| **Decision** | When an audit write fails, the business mutation **completes**. The failure is logged at error level and reported to the operator in the operation's result panel. The operation is not failed and not rolled back |
| **Why** | There is no transaction spanning the business mutation and the audit insert. Returning an error for a mutation that already succeeded would tell the operator the opposite of what happened — a false failure is less actionable than an honest "done, but not recorded". FR-AUD-08 requires the failure to be *surfaced*, which this satisfies |
| **Consequence** | Noye does **not** guarantee "no mutation without an audit row". An operator who ignores the warning has an unrecorded change. The stronger guarantee requires atomicity between the two writes and is tracked as RFC 0007 (`rfcs/proposed/007-atomic-audit-writes.md`) rather than assumed |
| **Re-evaluate when** | A compliance requirement demands that no change can occur without a record — or RFC 0007 is scheduled on its own merit. Note that the alternative was costed at roughly three additional days of cross-cutting refactor, not a configuration flip |
| **Closes** | Decision D-A |
| **Date** | 2026-07-28 |

### DEC-012 — Consecutive-count thresholds belong on the target

| Field | Content |
|---|---|
| **Decision** | `success_threshold` and `failure_threshold` move from `target_states` to `targets`. RFC 0008 accepted; scheduled for 0.29.0 alongside the configuration-import repair |
| **Why** | They are per-target *configuration*, not state. Every other decision criterion — expected status, body substring, TLS threshold, timeout, retries, interval — is already on `targets`, and FR-TGT-03 groups thresholds with them. Storing them on the state row is why they are absent from the configuration document and silently reset to 3 on an export/import round trip (gap G-06) |
| **Consequence** | `target_states` becomes fully derived — delete it and monitoring rebuilds it from the next check. `Target` gains two fields, so export and import carry them with no further work. Phase 4's range constraints must cover them: a threshold of 0 would mean "transition on no evidence" and must not be representable |
| **Re-evaluate when** | Never expected. If per-probe-type default thresholds are ever wanted, that is a separate question and does not depend on where the columns live |
| **Scheduling note** | Accepted *into* the repair phase rather than deferred out of it, unlike RFC 0007. The distinction is that RFC 0007 is independent of its repair while this one is a prerequisite — deferring it does not save the work, it schedules the import path to be built twice |
| **Date** | 2026-07-28 |

### DEC-013 — Suppression and SLA exclusion split into two flags

| Field | Content |
|---|---|
| **Decision** | `maintenance_windows` gains `exclude_from_sla`, independent of the existing `suppress_notify`, defaulting to 1. The interface presents three named situations — planned maintenance (silence + exclude), known external outage (silence, don't exclude), expected noise (exclude, don't silence) — rather than exposing the two flags directly |
| **Why** | One flag cannot express "silence alerts without forgiving the downtime," a case the requirements have named since the baseline (§13 D-2) — for example a known third-party outage that should not page anyone but should still count against measured availability. Under one flag, `suppress_notify = false` can only produce a window that does nothing, which is why closing G-07 with the existing schema was rejected: it would fix the bug and enshrine an incoherent flag |
| **Consequence** | FR-SUP-03 is restated as an exclusivity rule (target scope XOR tag scope XOR global) rather than a precedence rule, since Phase 3 makes the ambiguous state unrepresentable via a `CHECK` constraint. The SLA CSV's column 9 renames from `maintenance_seconds` to `excluded_seconds` — a breaking change to external interface I-08, versioned per external-design §14. FR-SUP-11's help text becomes conditional on which behaviours a given window has |
| **Re-evaluate when** | A third axis of window behaviour is proposed. Not expected — two flags cover every combination named against the requirements at this scale |
| **Closes** | §13 D-2, gaps G-07 and G-12 (scheduled; not yet implemented) |
| **Date** | 2026-07-28 |

### DEC-014 — Incident acknowledgement removed, not implemented

| Field | Content |
|---|---|
| **Decision** | `'acknowledged'` is removed from the `incidents.status` constraint. Acknowledgement is not implemented. Resolves D-4 per [RFC 0010](../../rfcs/proposed/010-incident-acknowledgement.md) |
| **Why** | The value is unreachable — nothing produces it, nothing reads it, no interface offers it. Implementing it properly means an acknowledged-at timestamp, an acknowledging actor, an audit action type, a defined interaction with notification suppression, and a queue affordance. That is a feature, not a constraint edit, and no requirement calls for it (P-1). The premise is a handful of operators for whom "has anyone seen this" is answered by asking |
| **Consequence** | Incident states are Open and Resolved, matching the glossary in §3. Phase 4's partial unique index covers `open` alone |
| **Re-evaluate when** | The incident queue routinely holds more open incidents than the team can hold in their heads, or more than one person works the queue concurrently without talking. At that point design it against a stated requirement — with the suppression interaction settled — rather than retrofitting a constraint value |
| **Date** | 2026-07-28 |

### DEC-015 — Interface scope adopted from the UI mockup

| Field | Content |
|---|---|
| **Decision** | Resolves D-B per [RFC 0011](../../rfcs/proposed/011-interface-integration.md). **Adopted wholesale:** design tokens, component semantics, progressive disclosure, copy. **Added as routes:** incident detail, channel detail, target statistics. **Deferred to `ROADMAP.md`:** trends, activity log, notification preferences, API tokens. **Rejected:** on-call status, on-shift landing, quiet hours, operations console, system console, tenant-scoped views |
| **Why** | The mockup's interaction design is genuinely better than what ships. Its scope drifted past the product into concepts already decided against — the rejected list is not a judgement on those ideas but a consequence of DEC-008 and DEC-009 |
| **Consequence** | Sixteen screens after Phase 6, not twenty-three. Adoption is by **re-expression into pure-function UI, never by merging code** — Leptos, axum and tokio do not compile for `wasm32-unknown-unknown` |
| **Re-evaluate when** | A deferred screen is asked for by an operator. Re-proposing a rejected one means reopening DEC-008 or DEC-009 with a fresh case |
| **Date** | 2026-07-28 |

### DEC-016 — Multilingual interface accepted

| Field | Content |
|---|---|
| **Decision** | NFR-I18N-01…04 stand. English and Japanese at launch. Resolves D-3 per [RFC 0009](../../rfcs/proposed/009-multilingual-interface.md). The mechanism is built in Phase 5; each screen converts as it is re-expressed in Phase 6 |
| **Why** | The development instructions have always required it, and leaving a requirement stated-but-unowned is the failure mode the lifecycle policy calls silent withdrawal. Sequencing matters more than the decision: string externalisation is far cheaper before the interface rebuild than after, so deciding late would mean touching every screen twice |
| **Consequence** | Interface strings only. Notification bodies, CSV headers, configuration keys, log output and audit `action_type` values stay untranslated — they are machine-facing contracts. Tests must assert on structure or string-table keys, never on translated display text |
| **Re-evaluate when** | Never expected. Adding a third language is a table entry, not a design change. Right-to-left layout would be its own RFC |
| **Date** | 2026-07-28 |

### DEC-017 — Retention batch size of 100, pending live verification

| Field | Content |
|---|---|
| **Decision** | `RETENTION_BATCH_SIZE = 100`, used as **both** the archive-select size and the delete-by-id chunk size. Shipped on M0 without verification against a live D1 instance |
| **Why** | D1's per-statement bound-parameter ceiling could not be checked — no Wrangler or D1 environment was available during implementation. 100 is the conservative commonly-cited figure. The two uses are deliberately coupled so one archived batch maps to exactly one `DELETE`: decoupling them (archive 1000, delete in chunks of 100) means a mid-chunk failure leaves rows archived but not deleted, and the next pass archives them again — violating DR-LIF-07 |
| **Consequence** | Both error directions are loud and fail-safe. **Too high**: D1 rejects the statement, the pass aborts, nothing is deleted. **Too low**: more R2 objects and more subrequests per invocation; at the documented scale (A-04) a steady-state pass exceeds one invocation's budget and resumes on the next tick, because deletion happens per batch. Neither direction silently loses data — which is what distinguishes this from G-20 |
| **Re-evaluate when** | Subject 36's deployment rehearsal measures both bounds against real D1: the actual bound-parameter ceiling, and the batches-per-invocation the subrequest budget allows. Close this decision then, with the measured numbers |
| **Where enforced** | `crates/core/src/db/retention.rs`; `rfcs/handoffs/02-retention-scope.md`; `rfcs/handoffs/36-release-rehearsal.md` |
| **Date** | 2026-07-28 |

### DEC-018 — The conformance-gap register is a governing artifact equivalent to an RFC

| Field | Content |
|---|---|
| **Decision** | For **conformance-gap remediation**, an entry in `docs/src/requirements.md` §11 is a governing artifact equivalent to an RFC. A Developer Handoff may derive from a gap entry, a decision record, or a requirement, as well as from an RFC. Every handoff states which, in a **Governing artifact** field |
| **Why** | The organisation policy frames handoffs as RFC-derived. 31 of 38 subjects remediate gaps found by the v0.27.2 independent review rather than implementing a new design. The gap register already carries what an RFC would — problem, the requirement violated, the consequence, remediation order — and, unlike a retroactive RFC, **each entry was verified against source when written**. Writing 31 RFCs after the fact would restate the decision log and add the ceremony the project's own lifecycle policy warns against for small projects |
| **Consequence** | A documented divergence from the baseline organisation policy, approved by the human owner under its §13. Traceability is preserved mechanically: every subject names its governing artifact, so roadmap → artifact → handoff → tests → evidence is followable without inference. `rfcs/handoffs/` is still not a lifecycle state |
| **Alternatives rejected** | Five umbrella RFCs per milestone — would largely duplicate DEC-008…DEC-017 and the requirement amendments. One RFC per subject — maximum conformance, near-zero information gain, and it would idle the implementer while written |
| **Re-evaluate when** | The gap register is exhausted and subjects derive predominantly from new design rather than remediation. At that point handoffs should be RFC-derived by default and this equivalence becomes vestigial |
| **Where enforced** | `rfcs/README.md`; `rfcs/handoffs/README.md`; the **Governing artifact** field on every subject |
| **Date** | 2026-07-28 |

---

## Security

| ID | Decision | Why it matters | Re-evaluate when |
|---|---|---|---|
| **SEC-001** | RUSTSEC-2023-0071 (`rsa`) suppressed with documented rationale | The crate is used only by the localhost dev IdP, which is never deployed. Keeps the audit signal honest without a false "clean" or a noisy block | Upstream ships a constant-time fix. Re-confirm quarterly |
| **SEC-002** | No plaintext secrets in the repository or release archive | Prevents credential leakage through the tarball | **Restored 2026-07-28** — was breached (NFR-SEC-09, gap G-21); closed by Subject 03 (NFR-SEC-14/15): neither `wrangler.toml` is tracked, and the `.example` templates carry no secret values |
| **SEC-003** | Auth, CSRF and crypto lean on audited upstreams and standard patterns | Avoids bespoke security code | A primitive we need has no maintained implementation |
| **SEC-004** | Dependency additions reviewed for licence, maintenance and security | Supply-chain hygiene | Never |
| **SEC-005** | Fail-closed Service-Binding token; dev-fallback rejection unconditional on environment | Private Core stays private; a leaked credential is refused regardless of what `NOYE_ENV` says | **Amended 2026-07-28 (Subject 03).** Until then, the fail-safe applied only when `NOYE_ENV` was *unset*, and the shipped configuration set it to `"development"`, disabling the guard entirely (gap G-21). `find_leaked_fallback` now takes no environment parameter in either crate — `NOYE_ENV` governs Gateway cookie strictness only and has no bearing on this control |
| **SEC-006** | The dependency scan is advisory on pull requests, blocking on push to `main` and on the weekly cron | A freshly-disclosed advisory should not block an unrelated PR. The cost is that a PR which *introduces* a vulnerable dependency also does not block — the CI run concludes `success` with the `cargo-audit` job red, and the scan only bites after merge or at the next cron. The design does not distinguish the two cases. Recorded 2026-07-29 after Subject 03b's T-166 run (`30460914785`) showed exactly this: the job failed, the run passed. Carried over from the v0.27.2 handoff's operational-risk table, which listed it and which this log omitted when it was built | Re-evaluate if a vulnerable dependency reaches `main` through a PR, or if the two cases can be distinguished — e.g. failing only on advisories affecting crates the PR itself changed | `.github/workflows/ci.yml` `continue-on-error`; `docs/src/development.md#continuous-integration` |

---

## How to add a decision here

A decision belongs in this log when a future contributor could
reasonably undo it without realising a choice was made. Record the
decision, the reason, the consequence to live with, **the criteria for
revisiting it**, and where it is enforced. A decision with no
re-evaluation criteria is an assumption; write one or admit it is
permanent.
