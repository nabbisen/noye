use noye_shared::CheckResult;
use wasm_bindgen::JsValue;
use worker::*;

pub async fn insert(db: &D1Database, result: &CheckResult) -> Result<()> {
    db.prepare(
        "INSERT INTO check_results
         (id, target_id, checked_at, is_success, status_code, response_time_ms,
          error_message, tls_expiry_date, tls_days_left, details)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )
    .bind(&[
        result.id.clone().into(),
        result.target_id.clone().into(),
        result.checked_at.clone().into(),
        JsValue::from(result.is_success as i32),
        result.status_code.map(JsValue::from).unwrap_or(JsValue::NULL),
        result.response_time_ms.map(JsValue::from).unwrap_or(JsValue::NULL),
        result.error_message.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        result.tls_expiry_date.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        result.tls_days_left.map(JsValue::from).unwrap_or(JsValue::NULL),
        result.details.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn list_recent(db: &D1Database, target_id: &str, limit: i64) -> Result<Vec<CheckResult>> {
    let results = db
        .prepare("SELECT * FROM check_results WHERE target_id = ?1 ORDER BY checked_at DESC LIMIT ?2")
        .bind(&[target_id.into(), JsValue::from(limit)])?
        .all()
        .await?;
    results.results::<CheckResult>()
}
