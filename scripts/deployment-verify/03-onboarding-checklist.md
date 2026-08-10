# Onboarding validation checklist

Subject 07a, Step 3, item #6 — `rfcs/handoffs/07a-live-residual-triage.md`.
Governing question: **is `docs/src/setup.md` sufficient on its own**,
followed by someone who has never deployed Noye before, against a
real, clean Cloudflare account? Nothing but a real account and a real
first-time run can answer that — this is not a script, because the
thing under test *is* a sequence of manual steps, and the only way to
validate a walkthrough is to walk through it.

**The rule, restated from the subject**: every undocumented step you
have to improvise is a documentation defect. Fix `docs/src/setup.md`
itself when you find one, not just your own run — a checklist that
notes "had to also run X" without adding X to the doc leaves the next
person to improvise the same thing.

## Before you start

- [ ] A genuinely clean Cloudflare account, or a clean sub-account /
      new zone if you're reusing one — the point is provisioning from
      nothing, not from a state that already has leftover resources
      that quietly cover for a missing step.
- [ ] Nothing pre-created. If `noye_db`, a `CACHE_KV` namespace, or a
      `noye-logs` bucket already exist from a prior attempt, either
      delete them first or note explicitly that you skipped a step
      because of that — otherwise a missing "create the D1 database"
      instruction could pass unnoticed.

## Walk through `docs/src/setup.md`, step by step

For each numbered step in the doc, record:

| Step | Doc says | What you actually did | Matched exactly? | Notes |
|---|---|---|---|---|
| 1. Toolchain | | | Y/N | |
| 2. Workspace verification | | | Y/N | |
| 3. Copy wrangler.toml templates | | | Y/N | |
| 4. Cloudflare resources (D1/KV/R2) | | | Y/N | |
| 5. D1 schema migration | | | Y/N | |
| 6. OIDC provider configuration | | | Y/N | |
| 7. Shared secret (Gateway↔Core) | | | Y/N | |
| 8. OIDC client secret | | | Y/N | |
| 9. Initial admin user | | | Y/N | |
| 10. Deploy | | | Y/N | |
| Smoke test | | | Y/N | |

"Matched exactly" means: the documented command, run verbatim, did
what the doc says it does, with no extra step, no extra flag, and no
research outside the doc required. Anything else goes in Notes —
quote the actual error or the actual extra step you had to take.

## Specific things worth checking deliberately

These are places the doc makes a claim that's easy to walk past
without actually verifying:

- [ ] **Step 6, OIDC provider.** The doc points to
      `oidc-providers.md` for provider-specific issuer URL formats.
      Pick a provider you have NOT already configured for this
      project before, and confirm that doc's format guidance for it
      is actually correct — this is the step most likely to be stale,
      since it depends on a third party's UI.
- [ ] **Step 7, shared secret.** Confirm the failure mode the doc
      describes is real: try a request between the two workers
      *before* setting the secret (or with mismatched values on each
      side) and confirm you get `FORBIDDEN`, not a confusing or
      silent failure.
- [ ] **Step 9, admin user email.** The doc says the email "must
      match the email claim issued by your OIDC provider for that
      account." Confirm this by trying to log in with an email that
      does *not* match first — does the failure mode make sense to
      someone who hasn't read this checklist?
- [ ] **Order dependency (Step 10).** The doc says Gateway must
      deploy after Core because of the Service Binding. Confirm this
      is still true by trying it in the wrong order once — capture
      the actual error `wrangler deploy` gives, so a future doc reader
      knows what they'll see if they get this wrong.
- [ ] **Migration numbering gap.** Step 5 mentions `0002` is
      intentionally retired and skipped. Confirm `wrangler d1
      migrations apply` doesn't warn about, or choke on, the gap.

## Capture form

Paste into the evidence log verbatim:

```
Cloudflare account state: <clean / reused, with details>
Date: <date>
Wrangler version: <wrangler --version>
Doc version tested: <git commit hash of docs/src/setup.md at time of test>

Step-by-step table: <the filled-in table above>

Deviations found (if any), each as:
  - Step:
  - What the doc says:
  - What actually happened:
  - Fix applied to docs/src/setup.md (commit or diff), or reason not fixed:

Specific checks above: <pass/fail + notes for each>

Overall: is docs/src/setup.md sufficient on its own for a first-time
deployer? <yes / no, with the deciding reason>
```

## What this checklist is not

It is not a substitute for reading `docs/src/setup.md` itself, and it
does not replace fixing the doc when you find a gap — a checklist
result that says "step 6 needed an undocumented extra flag" without a
corresponding edit to `setup.md` is a finding recorded and then left
to happen again to the next person.
