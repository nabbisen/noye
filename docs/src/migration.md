# Migration: moving a deployment

This document covers how to move a Noye deployment to a new Cloudflare account, replicate one environment into another (e.g. production → staging), or restore from a backup.

There are two layers to a Noye migration:

1. **Configuration** — the things humans configured: targets, channels, attachments, maintenance windows, optionally users. These are small (kilobytes) and slow to recreate by hand. Noye ships a UI for this at [`/admin/migration`](#configuration-migration).
2. **Bulk monitoring data** — the things Noye produced over time: `check_results`, `incidents`, `audit_logs`, R2 archive snapshots. These are large (potentially gigabytes) and high-volume. Use [`wrangler d1 export`](#bulk-monitoring-data) to move them.

Most migrations only need configuration data. The bulk-data step is for full disaster-recovery scenarios where the historical record needs to come along.

## Configuration migration

### Export

1. Sign in as an admin and visit `/admin/migration`.
2. Optionally tick **Include users**. This includes user emails, which are PII; off by default to make the export safe to share with other team members or hand over to a different operator.
3. Click **Download export**. The browser saves `noye-export-YYYYMMDD.json`.

The exported document is structured as:

```json
{
  "schema_version": 1,
  "exported_at": "2026-04-28T12:00:00Z",
  "source_deployment": "production",
  "data": {
    "targets": [...],
    "channels": [...],
    "target_notifications": [...],
    "maintenance_windows": [...],
    "users": [...]
  }
}
```

`source_deployment` reflects the optional `DEPLOYMENT_LABEL` env var on the Core, if set. Useful when you operate multiple Noye deployments and want exports to identify themselves.

The export is also available programmatically:

```bash
curl -H "Cookie: noye_session=..." \
  "https://noye.example.com/api/admin/migration/export?include_users=true" \
  > noye-export.json
```

### Import

1. Visit `/admin/migration` on the destination deployment, signed in as an admin.
2. Choose the JSON file from the source.
3. Pick a **conflict policy**:
   - **Skip** (default, recommended for fresh migrations) — keep existing rows; ignore incoming rows whose IDs collide.
   - **Replace** — overwrite existing rows with incoming data. Useful for restoring from a known-good backup.
   - **Fail** — refuse the entire import on the first ID collision. Useful when you expect a clean destination and want to be sure.
4. Decide whether to **Apply**:
   - Unchecked (default) — **dry run**: validate the payload and report row counts without writing anything.
   - Checked — actually write to D1.

Always run the dry-run first. The output shows you exactly what will be written.

### Validation

The import payload is validated structurally before any D1 work happens. The validator catches:

- Schema version mismatches (incoming `schema_version` not understood by the destination)
- Duplicate IDs within the payload
- Dangling foreign references (a `target_notifications` row pointing at a non-existent target or channel)
- Missing required fields (empty `id`, empty `name`, empty `host`)
- Unknown `channel_type` values

Validation errors are returned as a single response listing every problem, so you can fix the payload in one round.

When `users` are not included in the payload but `targets` or `channels` reference owners, you will see a warning advising you to ensure the destination already has user rows whose IDs match the referenced `owner_id` values. The import still succeeds.

### What gets carried across

| Table | In export? | Why |
|---|---|---|
| `targets` | Yes | Core configuration; rebuilding by hand is the main pain point this tool solves |
| `notification_channels` | Yes | Same reason |
| `target_notifications` (join) | Yes | The whole point of the configuration is the wiring |
| `maintenance_windows` | Yes | Both past and future; past windows preserve audit context |
| `users` | Optional | Contains emails; opt-in at export time |
| `check_results` | No | Volume; use `wrangler d1 export` |
| `incidents` | No | Volume; use `wrangler d1 export` |
| `audit_logs` | No | Volume + sensitive history; use `wrangler d1 export` |
| `target_states` | No | Reconstructible from the next Cron tick once monitoring resumes |
| KV (sessions, JWKS cache, rate-limit counters) | No | All of these regenerate naturally; copying them across accounts would be useless |
| R2 archive | No | Use bucket-to-bucket copy (see below) |

### Audit trail

Both export and import are recorded in the audit log:

- `resource_type=migration`, `action_type=export` — the operator and timestamp who exported, plus a detail string showing whether users were included and the row counts
- `resource_type=migration`, `action_type=import` — the operator, timestamp, conflict policy, and per-table counts (including `skipped` and `replaced`)

Search the audit log for these to track migration history.

## Bulk monitoring data

`wrangler d1 export` produces a SQL dump of every table; `wrangler d1 execute --file` plays it back. This is the right tool when you want to carry history.

```bash
# 1. On the source: dump every D1 table
wrangler d1 export noye_db --output noye-d1-dump.sql

# 2. On the destination: create the database (and apply migrations to get
#    the schema in place — this is what Cloudflare expects to see before
#    any data load):
wrangler d1 create noye_db_dest
cd crates/core && wrangler d1 migrations apply noye_db_dest && cd ../..

# 3. Apply the dump:
wrangler d1 execute noye_db_dest --file noye-d1-dump.sql
```

Caveats:

- `wrangler d1 export` does not have a row-limit option as of `wrangler@4`. Very large databases may need the `--no-data` and `--no-schema` flags to split structure from contents.
- The destination database ID will differ from the source. Update `database_id` in `crates/core/wrangler.toml` (or use `--env` to keep the source environment intact).

### R2 archive

The archive bucket holds older `check_results` rows snapshotted by the retention pass. To mirror it:

```bash
# Using rclone (recommended; supports S3-compatible R2 endpoints)
rclone sync r2-source:noye-logs r2-dest:noye-logs

# Or wrangler:
wrangler r2 object get noye-logs/<key> --file local.json
wrangler r2 object put noye-logs/<key> --file local.json --bucket noye-logs-dest
# (loop over keys with `wrangler r2 object list`)
```

The wrangler-only path is awkward for large buckets; rclone (or `aws s3 sync` with R2's S3-compatible endpoint) is the practical choice.

## Migration playbooks

### Account-to-account migration

1. Provision new D1, KV, and R2 resources on the destination account. Run schema migrations.
2. Register every secret on the destination (`GATEWAY_SHARED_TOKEN`, `OIDC_CLIENT_SECRET`, `EMAIL_SMTP_PASSWORD` if applicable). Generate fresh values; do not reuse the source's secrets.
3. Update `crates/gateway/wrangler.toml` and `crates/core/wrangler.toml` on the destination with the new resource IDs.
4. Deploy Core first, then Gateway (see [`deployment.md`](deployment.md#deploy-order)).
5. Configure OIDC on the IdP to allow the new redirect URI.
6. **Configuration migration** via `/admin/migration` (export from source, import on destination with `Skip` policy).
7. **Bulk monitoring data** via `wrangler d1 export` if needed.
8. Verify monitoring is running on the destination (Cron should produce new `check_results` within 60 seconds).
9. Once verified, point your DNS at the new Gateway and decommission the source.

### Production → staging clone

1. From production, export configuration with `include_users = false` (you don't want production user emails seeded into staging).
2. On staging, import with `Skip` policy. Add a couple of test-purpose users via the Settings page after.
3. Don't bring bulk data over — staging should accumulate its own.

### Restore from backup

1. Restore D1 with `wrangler d1 execute --file <backup>.sql`. This includes both schema and data.
2. The configuration migration UI is not needed for this scenario — `wrangler d1 execute` already replays everything.
3. After restore, the next Cron tick rebuilds `target_states`. Notifications won't fire for state changes that happened during the outage; if that matters, manually inspect `incidents` for the gap and decide whether to alert recipients.

## See also

- [`deployment.md`](deployment.md) — pre-flight, deploy order, environments
- [`deployment-secrets.md`](deployment-secrets.md) — secret rotation that's required during account-to-account migrations
- [`api.md`](api.md) — full API reference for `/api/admin/migration/*`
