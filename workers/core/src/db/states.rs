use noye_shared::TargetState;
use wasm_bindgen::JsValue;
use worker::*;

/// 状態遷移の結果
#[derive(Debug, Clone)]
pub struct TransitionResult {
    #[allow(dead_code)]
    pub target_id: String,
    pub previous_status: String,
    pub new_status: String,
    pub changed: bool,
}

pub async fn list_all(db: &D1Database) -> Result<Vec<TargetState>> {
    let results = db.prepare("SELECT * FROM target_states").bind(&[])?.all().await?;
    results.results::<TargetState>()
}

pub async fn get_by_target(db: &D1Database, target_id: &str) -> Result<TargetState> {
    db.prepare("SELECT * FROM target_states WHERE target_id = ?1")
        .bind(&[target_id.into()])?
        .first::<TargetState>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("State not found for target: {}", target_id)))
}

/// チェック結果に基づいて状態を更新する。
/// 連続成功/失敗回数を加算し、しきい値に達した時点で状態遷移させる。
pub async fn update_after_check(
    db: &D1Database,
    target_id: &str,
    is_success: bool,
) -> Result<TransitionResult> {
    let state = get_by_target(db, target_id).await?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let previous_status = state.current_status.clone();
    let (new_successes, new_failures) = if is_success {
        (state.consecutive_successes + 1, 0_i64)
    } else {
        (0_i64, state.consecutive_failures + 1)
    };

    let new_status = if new_failures >= state.failure_threshold && previous_status != "down" {
        "down".to_string()
    } else if new_successes >= state.success_threshold && previous_status == "down" {
        "up".to_string()
    } else if is_success && previous_status == "unknown" {
        "up".to_string()
    } else {
        previous_status.clone()
    };

    let changed = new_status != previous_status;
    let status_change_at = if changed {
        now.clone()
    } else {
        state.last_status_change_at.unwrap_or_else(|| now.clone())
    };

    db.prepare(
        "UPDATE target_states SET current_status = ?1, consecutive_successes = ?2,
         consecutive_failures = ?3, last_checked_at = ?4, last_status_change_at = ?5
         WHERE target_id = ?6",
    )
    .bind(&[
        new_status.clone().into(),
        JsValue::from(new_successes),
        JsValue::from(new_failures),
        now.into(),
        status_change_at.into(),
        target_id.into(),
    ])?
    .run()
    .await?;

    Ok(TransitionResult {
        target_id: target_id.to_string(),
        previous_status,
        new_status,
        changed,
    })
}

pub async fn mark_notified(db: &D1Database, target_id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    db.prepare("UPDATE target_states SET last_notification_at = ?1 WHERE target_id = ?2")
        .bind(&[now.into(), target_id.into()])?
        .run()
        .await?;
    Ok(())
}
