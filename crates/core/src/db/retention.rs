use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::*;

/// Data lifecycle management (requirement 3)
///
/// Automatically delete or archive to R2 any records past their retention period.
/// Invoked periodically from the Cron Trigger.

#[derive(Debug, Deserialize)]
struct RetentionPolicy {
    table_name: String,
    retention_days: i64,
    archive_to_r2: bool,
    #[allow(dead_code)]
    last_cleanup_at: Option<String>,
}

/// D1 bounds the number of parameters a single prepared statement may bind.
/// This value is deliberately used as *both* the archive-select batch size
/// and the delete-by-id chunk size, so one archived batch maps to exactly
/// one `DELETE`, with no sub-chunking and therefore no way for the
/// archived set and the deleted set to drift apart from a partially
/// applied chunk.
///
/// This limit could not be verified against a live D1 instance while
/// implementing this fix (no Wrangler / D1 environment available). If a
/// live check finds the true limit is lower, lower this constant — the
/// archived-set-equals-deleted-set property must hold regardless of the
/// number chosen. See rfcs/handoffs/02-retention-scope.md "Stop and
/// report".
const RETENTION_BATCH_SIZE: i64 = 100;

/// The `WHERE` clause selecting rows eligible for retention processing in
/// `table_name`, or `None` for a table the retention system does not
/// (yet) know how to process.
///
/// Single source of truth for eligibility, shared by the archive-select
/// and delete-by-id steps, so the two can never independently drift on
/// which rows qualify. `?1` is the bound cutoff timestamp in every case.
///
/// `table_name` is later interpolated directly into SQL text (D1 has no
/// bind-parameter form for identifiers); this is safe only because every
/// caller reaches it exclusively through this function, and this match's
/// arms are the entire allowlist of literals it can produce. Same pattern
/// as `db::migration::exists_by_id` (see docs/src/security-posture.md).
fn eligibility_where_clause(table_name: &str) -> Option<&'static str> {
    match table_name {
        "check_results" => Some("checked_at < ?1"),
        "incidents" => Some("opened_at < ?1 AND status = 'resolved'"),
        "audit_logs" => Some("action_time < ?1"),
        _ => None,
    }
}

/// Extract each row's `id` field as a `String`, failing loudly if any row
/// lacks one. Pure and host-testable (NFR-QA-01) — the async D1 calls
/// around it are the only part that need a Worker runtime.
fn extract_ids(rows: &[serde_json::Value]) -> std::result::Result<Vec<String>, String> {
    rows.iter()
        .map(|row| {
            row.get("id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| format!("retention: row missing string 'id' field: {row}"))
        })
        .collect()
}

/// Whether DR-LIF-02 / DR-LIF-03 require archival before deletion for
/// `table_name`. For these classes, `archive_to_r2 = 0` in
/// `retention_policies` is not a valid configuration to honour — it would
/// recreate G-20's consequence (unarchived deletion) through
/// configuration rather than through a bug. See
/// `rfcs/handoffs/02-retention-scope.md` Build step 4.
///
/// `audit_logs` is deliberately excluded: no current requirement makes
/// archival-before-deletion a precondition for it (its retention-deletion
/// behaviour changes independently in subject 04).
fn requires_archival(table_name: &str) -> bool {
    matches!(table_name, "check_results" | "incidents")
}

/// Run a retention-period cleanup pass.
///
/// For each policy: repeatedly select up to `RETENTION_BATCH_SIZE`
/// eligible rows, archive that exact batch (if the policy calls for it),
/// then delete that exact batch by id — never a separate, unbounded
/// `DELETE`. Deleting per batch, rather than accumulating to the end of
/// the pass, is what makes a timed-out Worker invocation resumable: any
/// batch already archived-and-deleted does not reappear next run, and any
/// not-yet-processed batch remains eligible.
pub async fn run_cleanup(env: &Env) -> Result<()> {
    let db = env.d1("DB")?;
    let now = chrono::Utc::now();
    let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let policies = db
        .prepare("SELECT * FROM retention_policies")
        .bind(&[])?
        .all()
        .await?
        .results::<RetentionPolicy>()?;

    for policy in &policies {
        let Some(where_clause) = eligibility_where_clause(&policy.table_name) else {
            console_error!(
                "retention: retention_policies names table '{}', which the retention \
                 system does not recognize; skipping it rather than silently doing nothing",
                policy.table_name
            );
            continue;
        };

        if requires_archival(&policy.table_name) && !policy.archive_to_r2 {
            console_error!(
                "retention: policy for '{}' has archive_to_r2 = 0, but this class \
                 requires archival before deletion (DR-LIF-02 / DR-LIF-03); skipping \
                 rather than deleting unarchived records. Fix the policy row.",
                policy.table_name
            );
            continue;
        }

        let cutoff_str = compute_cutoff(now, policy.retention_days);

        loop {
            let batch = select_eligible_batch(
                &db,
                &policy.table_name,
                where_clause,
                &cutoff_str,
                RETENTION_BATCH_SIZE,
            )
            .await?;
            if batch.is_empty() {
                break;
            }

            let ids = extract_ids(&batch).map_err(Error::RustError)?;

            if policy.archive_to_r2 {
                archive_batch(env, &policy.table_name, &batch).await?;
            }

            delete_by_ids(&db, &policy.table_name, &ids).await?;
        }

        db.prepare("UPDATE retention_policies SET last_cleanup_at = ?1 WHERE table_name = ?2")
            .bind(&[now_str.clone().into(), policy.table_name.clone().into()])?
            .run()
            .await?;
    }

    Ok(())
}

/// Select up to `limit` rows from `table_name` matching `where_clause`,
/// bound to `cutoff` as `?1`. Returns full rows (not just ids) so a
/// single query serves both the archive step and, via [`extract_ids`],
/// the delete step — the two can never see a different row set.
async fn select_eligible_batch(
    db: &D1Database,
    table_name: &str,
    where_clause: &str,
    cutoff: &str,
    limit: i64,
) -> Result<Vec<serde_json::Value>> {
    let sql = format!("SELECT * FROM {table_name} WHERE {where_clause} LIMIT ?2");
    db.prepare(&sql)
        .bind(&[cutoff.into(), JsValue::from(limit)])?
        .all()
        .await?
        .results::<serde_json::Value>()
}

/// Archive exactly `rows` to R2 as a single JSON array. Failure is
/// propagated with `?` to the caller, which must not delete the batch
/// this call was given if it fails.
async fn archive_batch(env: &Env, table_name: &str, rows: &[serde_json::Value]) -> Result<()> {
    let bucket = env.bucket("LOG_BUCKET")?;

    let archive_json = serde_json::to_string(rows)
        .map_err(|e| Error::RustError(format!("Archive serialization error: {}", e)))?;

    let now = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let key = format!("archive/{}/{}_{}.json", table_name, now, rows.len());

    bucket
        .put(&key, worker::Data::Text(archive_json))
        .execute()
        .await?;

    console_log!(
        "Archived {} records from {} to R2: {}",
        rows.len(),
        table_name,
        key
    );
    Ok(())
}

/// Delete exactly the rows identified by `ids` from `table_name`. `ids`
/// is bounded by `RETENTION_BATCH_SIZE`, so the bind list here is bounded
/// too — this is never an unbounded `DELETE`.
async fn delete_by_ids(db: &D1Database, table_name: &str, ids: &[String]) -> Result<()> {
    if ids.is_empty() {
        return Ok(());
    }

    let placeholders: Vec<String> = (1..=ids.len()).map(|i| format!("?{i}")).collect();
    let sql = format!(
        "DELETE FROM {table_name} WHERE id IN ({})",
        placeholders.join(", ")
    );
    let binds: Vec<JsValue> = ids.iter().map(|id| JsValue::from(id.as_str())).collect();

    db.prepare(&sql).bind(&binds)?.run().await?;
    Ok(())
}

/// Compute the retention cutoff as an ISO-8601 UTC timestamp.
///
/// Records older than this timestamp fall outside the retention window and
/// become eligible for archival or deletion. Extracted as a pure helper so
/// the date-math is unit-testable without a Worker `Env` or D1 binding.
fn compute_cutoff(now: chrono::DateTime<chrono::Utc>, retention_days: i64) -> String {
    let cutoff = now - chrono::Duration::days(retention_days);
    cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests;
