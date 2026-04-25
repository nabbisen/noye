use serde::Deserialize;
use worker::*;

/// データライフサイクル管理 (要件3)
///
/// 保持期間を超えたレコードを自動的に削除またはR2にアーカイブする。
/// Cron Trigger から定期的に呼び出される。

#[derive(Debug, Deserialize)]
struct RetentionPolicy {
    table_name: String,
    retention_days: i64,
    archive_to_r2: bool,
    #[allow(dead_code)]
    last_cleanup_at: Option<String>,
}

/// 保持期間を超えたデータのクリーンアップを実行する
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
        let cutoff = now - chrono::Duration::days(policy.retention_days);
        let cutoff_str = cutoff.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        // R2 へのアーカイブが必要な場合
        if policy.archive_to_r2 {
            archive_old_records(env, &policy.table_name, &cutoff_str).await?;
        }

        // 対象テーブルに応じた削除
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

        // クリーンアップ実行時刻を記録
        db.prepare(
            "UPDATE retention_policies SET last_cleanup_at = ?1 WHERE table_name = ?2",
        )
        .bind(&[now_str.clone().into(), policy.table_name.clone().into()])?
        .run()
        .await?;
    }

    Ok(())
}

/// 古いレコードをR2にJSON形式でアーカイブする
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
