# Deployment: observability

Noye monitors other servers — but who monitors Noye? This document covers how to observe the health of a running Noye deployment.

## What's already instrumented

Noye emits operational data through three channels by default. None of them require additional setup beyond a deployed and running pair of workers.

### Cloudflare dashboard metrics

Every Worker shows the following automatically in the Cloudflare dashboard under **Workers & Pages → \<worker name\> → Metrics**:

- Request count (per minute, hour, day)
- CPU time (median, p99)
- Errors (4xx and 5xx response counts)
- Bandwidth in and out

For the Gateway: this tells you how much UI traffic the system is carrying and whether anything is failing.

For the Core: requests are dominated by Service Binding calls from the Gateway plus Cron-triggered scheduled events. A sudden drop in request count on the Core is a strong signal that the Cron is failing.

The dashboard's per-worker metrics retain 24 hours on the free plan and 7 days on Workers Paid. For longer retention, ship to an external observability backend (see "Sending metrics elsewhere" below).

### `console_log!` and `console_error!`

Noye uses these macros liberally. They land in two places:

1. **Real-time** via `wrangler tail` — see [deployment-troubleshooting.md](deployment-troubleshooting.md#diagnostic-starting-points).
2. **Persisted** in Cloudflare's Logpush feature (Workers Paid plan) — every `console.*` call is captured. You can route them to R2, S3, or any HTTP endpoint.

The conventions in the codebase:

- `console_log!` for routine progress (e.g. `"Cleaned up N rows from check_results"`)
- `console_error!` for exceptions that don't fail the request but indicate something needs attention (e.g. failed notification dispatch)

A small but valuable practice: search the codebase before adding a new `console_log!` to make sure the message is unique. Greppable log messages are how you correlate signals across Logpush dumps months later.

### Audit log

The `audit_logs` table is the highest-fidelity record of what humans (or the system itself) actually did. It is queryable via SQL on D1:

```bash
# Last 50 actions
wrangler d1 execute noye_db --command \
  "SELECT action_time, actor_email, resource_type, action_type, result
   FROM audit_logs ORDER BY action_time DESC LIMIT 50"

# All test-send activity in the last 24 hours
wrangler d1 execute noye_db --command \
  "SELECT action_time, actor_email, resource_id, new_value
   FROM audit_logs
   WHERE action_type = 'test_send'
     AND action_time > datetime('now', '-1 day')
   ORDER BY action_time DESC"

# Who deleted what, ever
wrangler d1 execute noye_db --command \
  "SELECT action_time, actor_email, resource_type, resource_id
   FROM audit_logs WHERE action_type = 'delete'"
```

The `/audit` page surfaces the same data but is restricted to admins and capped at 200 rows; raw SQL is the right tool for forensic work.

### Verifying audit-log integrity

Audit rows are linked together by a SHA-256 hash chain (since 0.27.2). Tampering with any row by `UPDATE` / `DELETE` / out-of-order insertion breaks the chain at that row and at every subsequent row, surfaced by the verifier:

```bash
# Admin session cookie required; example shown via curl with a saved cookie jar
curl -s -b cookies.txt https://noye-gateway.example.com/api/admin/audit/verify | jq
```

Sample healthy output:

```json
{
  "total_rows": 1234,
  "legacy_rows": 0,
  "verified_rows": 1234,
  "tampered_rows": []
}
```

Sample report after a tampered row:

```json
{
  "total_rows": 1234,
  "legacy_rows": 0,
  "verified_rows": 999,
  "tampered_rows": [
    {
      "id": "abc-123",
      "action_time": "2026-04-15T08:12:33Z",
      "reason": "row_hash does not match recomputed value (row contents tampered)"
    }
  ]
}
```

`legacy_rows` are rows written before 0.27.2 (NULL hash columns) — they are not part of any chain and are reported separately, not as tampering.

Run the verification on a schedule (e.g. weekly) and alert if `tampered_rows` is non-empty. Any non-zero count indicates either:

- A direct `wrangler d1 execute "UPDATE audit_logs ..."` call (operator action that should be reviewed).
- A row deleted from the middle of the chain.
- A bug in `crates/core/src/db/audit/hash.rs` (test it locally — the 21-test suite there is the reference).

For the scope of what this catches and what it does not, see [security-posture.md](security-posture.md#audit-log-tamper-detection).

## Health checks: is monitoring still working?

The metric you care about most is "is Noye actually monitoring." Two complementary checks:

### Liveness of the Cron pipeline

```sql
-- Last 5 minutes of check activity
SELECT COUNT(*) AS recent_checks,
       MAX(checked_at) AS most_recent
FROM check_results
WHERE checked_at > datetime('now', '-5 minutes');
```

`recent_checks = 0` for more than five minutes means the Cron is not draining the queue. Causes are listed in the troubleshooting guide; the most likely ones are a Core deployment failure or a D1 connectivity issue.

### Coverage check

```sql
-- Targets that are due for a check but have not been checked recently
SELECT name, host, next_check_at, last_checked_at
FROM targets t
LEFT JOIN target_states s ON t.id = s.target_id
WHERE t.is_disabled = 0
  AND t.next_check_at < datetime('now')
  AND (s.last_checked_at IS NULL
       OR s.last_checked_at < datetime('now', '-' || (t.interval_minutes * 2) || ' minutes'))
ORDER BY t.next_check_at;
```

This query returns rows when a target's `next_check_at` has passed but the actual check hasn't run for at least twice its check interval — a sign that this specific target is being skipped. The most common cause is a target whose `interval_minutes` is shorter than what the Cron can drain in one minute, paired with a backlog elsewhere.

### Self-monitoring with Noye

A satisfying way to validate that Noye works is to monitor the Gateway's `/healthz` endpoint *with* Noye:

1. Add a target of type `https`, host `<gateway-domain>`, path `/healthz`, expected status `200`.
2. Configure a Slack or webhook notification channel to a destination separate from where most of your alerts flow.
3. Attach the channel to the new target.

If Noye breaks, this target's notifications won't fire — which is itself a signal because the destination (a personal Slack channel, for instance) is a place where the absence of a regular up signal is noticeable.

This is not a substitute for an external uptime check (you cannot use a thing to monitor itself for outages), but it is a cheap sanity check that catches more failure modes than you might expect.

## Operational queries

A handful of D1 queries that come up repeatedly:

### Targets that have been down for hours

```sql
SELECT t.name, t.host, ts.current_status, ts.last_status_change_at
FROM targets t
JOIN target_states ts ON t.id = ts.target_id
WHERE ts.current_status = 'down'
  AND ts.last_status_change_at < datetime('now', '-2 hours')
ORDER BY ts.last_status_change_at;
```

Use to spot targets that have been ignored — either no one is responding to alerts or the channel is broken.

### Active maintenance windows right now

```sql
SELECT name, start_at, end_at, target_tag, target_id
FROM maintenance_windows
WHERE is_active = 1
  AND start_at <= datetime('now')
  AND end_at >= datetime('now');
```

If notifications are unexpectedly silent, check this first.

### Most active users

```sql
SELECT actor_email, COUNT(*) AS actions
FROM audit_logs
WHERE action_time > datetime('now', '-7 days')
GROUP BY actor_email
ORDER BY actions DESC;
```

Useful for permission reviews.

### Recent failed test sends

```sql
SELECT action_time, actor_email, resource_id AS channel_id, new_value
FROM audit_logs
WHERE action_type = 'test_send'
  AND new_value LIKE 'error:%'
ORDER BY action_time DESC
LIMIT 20;
```

If multiple test sends have failed against the same channel, the channel's endpoint is misconfigured.

## Sending metrics elsewhere

For deployments where you want metrics outside the Cloudflare dashboard:

### Logpush (Workers Paid plan)

Logpush captures every request and every `console.*` call and ships them to R2, S3, or an HTTP endpoint. Configuration is done through the Cloudflare dashboard or API, not `wrangler.toml`. The output format is JSONL with one event per line.

This is the easiest path to a long-term log archive and to feeding signals into Splunk, Datadog, or similar.

### Custom metric beacons

For a lightweight alternative, you can emit metric points directly from the Workers code using `fetch()` to a metrics ingest endpoint. The `monitor::engine::run_scheduled_checks` function is a natural place to emit a "Cron tick" metric:

```rust
let _ = fetch_post(
    env.var("METRICS_ENDPOINT")?.to_string(),
    serde_json::json!({
        "metric": "noye.cron.tick",
        "ts": chrono::Utc::now().timestamp(),
        "checks_run": count,
    }),
).await;
```

This is not implemented in 0.5.0 but is straightforward to add when you want it. Keep the budget tight (one metric per Cron tick is cheap; one metric per check is not).

## Alerting

Noye does not alert on its own health by design — it alerts on the health of the *targets* it watches. To alert on Noye itself, the practical patterns are:

- **External uptime monitoring of the Gateway's `/healthz`** with whatever your team already uses (Pingdom, UptimeRobot, BetterStack, an internal monitor, etc.). If Noye is your only monitoring tool, this becomes a chicken-and-egg problem; pick a different vendor for this one check.
- **Cloudflare alerts on Workers errors.** The dashboard supports email or webhook alerts when error rate crosses a threshold. Configure on `Notifications → Workers → New alert`.
- **Audit-log SQL queries on a schedule.** A small periodic job that runs the "targets down for hours" query and pages a human if the result set is too large.

Alerting policy is necessarily site-specific; the building blocks above are what's available.
