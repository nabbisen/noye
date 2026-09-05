# Provisioning fixtures

## `d1-info.json`

**Not captured from a real Cloudflare account** — standing rule 7 keeps
that outside the dev team's reach; `provision`'s automated tests never
touch real infrastructure (subject 37).

Reconstructed from `wrangler` 4.97.0's own bundled source
(`getDatabaseInfoFromIdOrName` in `src/d1/utils.ts`, which returns the
Cloudflare D1 REST API's `/accounts/:id/d1/database/:id` response
verbatim under `--json`) and the D1 API's documented response shape.

Only the `"uuid"` field is load-bearing for
`ids::parse_d1_database_id` — every other field is illustrative, not
verified against a real response, and the parser ignores them entirely
so drift in fields we don't use can't break id resolution. Report any
mismatch found against a real account during the deployment session
(subject 37's Escalate section names the same caveat for KV namespace
id resolution, which has no `--json` output to fix a fixture against at
all).
