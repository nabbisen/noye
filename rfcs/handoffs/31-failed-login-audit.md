# 31 — Failed-login audit recording

**Milestone** M5 · **Satisfies** FR-AUTH-10
**Implements** [RFC 0004](../proposed/004-failed-login-audit.md)
**Branch** `rfc-0004-failed-login` · **Depends on** M4 shipped
**Governing artifact** — **RFC 0004**

## Scope

The OIDC callback records only *successful* logins. Failures go to
`console_error!` and appear neither in `/me/security` nor in the chain.

## Build

Per RFC 0004. The design question it flags — what to attribute a failure
to, when the attempted identity may not correspond to a real user — is
already answered by subject 06's snapshot actor.

Record the claimed identity with an explicit `trusted` marker
distinguishing **verified** from **claimed**, as FR-AUTH-10 requires.
That distinction is the entire point: an audit row asserting an identity
nobody verified is worse than no row.

### ⛔ Never record a credential

Not a password, not a token, not an authorization code. A failed-login
record is attractive to log verbosely and is exactly where a secret ends
up in a database by accident.

## Verify

| # | Test | Type |
|---|---|---|
| T-147 | A failed login writes an audit row carrying a `trusted` marker | **must fail first** |
| T-148 | That row contains no credential, token or password | **guard — critical** |
| T-149 | Failed-login rows appear in the user's own recent-logins view | **must fail first** |
| T-150 | A failure for an unknown identity still records, marked as claimed | **must fail first** |

**T-148 must use a fixture submitting a recognisable secret** and assert
its absence from the stored row and from any log line.

## Done

- All four tests pass; three baseline failures captured
- RFC 0004 → `rfcs/done/`, `Status: Implemented (1.0.0)`, inbound links fixed
- `docs/src/requirements.md`: FR-AUTH-10 → `Implemented`

## Escalate

T-148 failing at any point → requirements architect, immediately.
