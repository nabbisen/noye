use noye_shared::{AuditEntry, Caller};
use wasm_bindgen::JsValue;
use worker::*;

pub async fn log(
    db: &D1Database,
    caller: &Caller,
    resource_type: &str,
    resource_id: &str,
    action_type: &str,
    previous_value: Option<&str>,
    new_value: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO audit_logs
         (id, action_time, actor_id, actor_email, resource_type, resource_id,
          action_type, previous_value, new_value, result)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'success')",
    )
    .bind(&[
        id.into(), now.into(),
        caller.user_id.clone().into(),
        caller.email.clone().into(),
        resource_type.into(), resource_id.into(), action_type.into(),
        previous_value.map(JsValue::from).unwrap_or(JsValue::NULL),
        new_value.map(JsValue::from).unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn log_system(
    db: &D1Database,
    resource_type: &str,
    resource_id: &str,
    action_type: &str,
    details: Option<&str>,
) -> Result<()> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO audit_logs
         (id, action_time, actor_id, actor_email, resource_type, resource_id,
          action_type, new_value, result)
         VALUES (?1, ?2, 'system', 'system', ?3, ?4, ?5, ?6, 'success')",
    )
    .bind(&[
        id.into(), now.into(),
        resource_type.into(), resource_id.into(), action_type.into(),
        details.map(JsValue::from).unwrap_or(JsValue::NULL),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn list_recent(db: &D1Database, limit: i64) -> Result<Vec<AuditEntry>> {
    let results = db
        .prepare("SELECT * FROM audit_logs ORDER BY action_time DESC LIMIT ?1")
        .bind(&[JsValue::from(limit)])?
        .all()
        .await?;
    results.results::<AuditEntry>()
}
