use serde::Deserialize;
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

/// Run a retention-period cleanup pass
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
        let cutoff_str = compute_cutoff(now, policy.retention_days);

        // When archival to R2 is required
        if policy.archive_to_r2 {
            archive_old_records(env, &policy.table_name, &cutoff_str).await?;
        }

        // Per-table deletion
        let delete_sql = match policy.table_name.as_str() {
            "check_results" => {
                "DELETE FROM check_results WHERE checked_at < ?1"
            }
            "incidents" => {
                "DELETE FROM incidents WHERE opened_at < ?1 AND status = 'resolved'"
            }
            "audit_logs" => {
                "DELETE FROM audit_logs WHERE action_time < ?1"
            }
            _ => continue,
        };

        db.prepare(delete_sql)
            .bind(&[cutoff_str.into()])?
            .run()
            .await?;

        // Record the time of the last cleanup run
        db.prepare(
            "UPDATE retention_policies SET last_cleanup_at = ?1 WHERE table_name = ?2",
        )
        .bind(&[now_str.clone().into(), policy.table_name.clone().into()])?
        .run()
        .await?;
    }

    Ok(())
}

/// Archive old records to R2 in JSON format
async fn archive_old_records(env: &Env, table_name: &str, cutoff: &str) -> Result<()> {
    let db = env.d1("DB")?;
    let bucket = env.bucket("LOG_BUCKET")?;

    let query = match table_name {
        "check_results" => {
            format!("SELECT * FROM check_results WHERE checked_at < '{}' LIMIT 1000", cutoff)
        }
        "incidents" => {
            format!(
                "SELECT * FROM incidents WHERE opened_at < '{}' AND status = 'resolved' LIMIT 1000",
                cutoff
            )
        }
        "audit_logs" => {
            format!("SELECT * FROM audit_logs WHERE action_time < '{}' LIMIT 1000", cutoff)
        }
        _ => return Ok(()),
    };

    let results = db
        .prepare(&query)
        .bind(&[])?
        .all()
        .await?;

    let raw = results.results::<serde_json::Value>()?;
    if raw.is_empty() {
        return Ok(());
    }

    let archive_json = serde_json::to_string(&raw)
        .map_err(|e| Error::RustError(format!("Archive serialization error: {}", e)))?;

    let now = chrono::Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let key = format!("archive/{}/{}_{}.json", table_name, now, raw.len());

    bucket
        .put(&key, worker::Data::Text(archive_json))
        .execute()
        .await?;

    console_log!("Archived {} records from {} to R2: {}", raw.len(), table_name, key);
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
mod tests {
    use super::compute_cutoff;
    use chrono::TimeZone;

    fn anchor() -> chrono::DateTime<chrono::Utc> {
        // 2026-04-01T00:00:00Z
        chrono::Utc.with_ymd_and_hms(2026, 4, 1, 0, 0, 0).unwrap()
    }

    #[test]
    fn ninety_days_ago_at_midnight() {
        // 90 days before 2026-04-01 is 2026-01-01 (90 days = Jan 1 to Apr 1 in 2026, non-leap year)
        let cutoff = compute_cutoff(anchor(), 90);
        assert_eq!(cutoff, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn one_day_ago() {
        let cutoff = compute_cutoff(anchor(), 1);
        assert_eq!(cutoff, "2026-03-31T00:00:00Z");
    }

    #[test]
    fn zero_days_ago_is_now() {
        let cutoff = compute_cutoff(anchor(), 0);
        assert_eq!(cutoff, "2026-04-01T00:00:00Z");
    }

    #[test]
    fn cutoff_format_is_iso_8601_utc_z() {
        // Format must end with 'Z' (Zulu/UTC) and use 'T' as the date/time separator.
        // SQL comparisons against opened_at / action_time / checked_at depend on this.
        let cutoff = compute_cutoff(anchor(), 30);
        assert!(cutoff.ends_with('Z'));
        assert_eq!(cutoff.matches('T').count(), 1);
        assert_eq!(cutoff.len(), 20); // YYYY-MM-DDTHH:MM:SSZ
    }

    #[test]
    fn cutoff_preserves_time_of_day() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 4, 15, 13, 45, 30).unwrap();
        let cutoff = compute_cutoff(now, 7);
        assert_eq!(cutoff, "2026-04-08T13:45:30Z");
    }
}
