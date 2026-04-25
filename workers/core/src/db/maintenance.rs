use noye_shared::{Caller, CreateMaintenanceInput, MaintenanceWindow};
use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::*;

pub async fn create(
    db: &D1Database,
    input: &CreateMaintenanceInput,
    caller: &Caller,
) -> Result<MaintenanceWindow> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO maintenance_windows
         (id, name, start_at, end_at, target_tag, target_id, suppress_notify,
          is_active, created_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1, ?8, ?9, ?10)",
    )
    .bind(&[
        id.clone().into(),
        input.name.clone().into(),
        input.start_at.clone().into(),
        input.end_at.clone().into(),
        input.target_tag.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        input.target_id.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        JsValue::from(input.suppress_notify.unwrap_or(true) as i32),
        now.into(),
        caller.user_id.clone().into(),
        caller.user_id.clone().into(),
    ])?
    .run()
    .await?;

    get_by_id(db, &id).await
}

pub async fn list_active(db: &D1Database) -> Result<Vec<MaintenanceWindow>> {
    let results = db
        .prepare("SELECT * FROM maintenance_windows WHERE is_active = 1 ORDER BY start_at DESC")
        .bind(&[])?
        .all()
        .await?;
    results.results::<MaintenanceWindow>()
}

pub async fn is_under_maintenance(db: &D1Database, target_id: &str, tags: Option<&str>) -> Result<bool> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let result = db
        .prepare(
            "SELECT COUNT(*) as cnt FROM maintenance_windows
             WHERE is_active = 1 AND start_at <= ?1 AND end_at >= ?1
             AND (target_id = ?2
                  OR (target_tag IS NOT NULL AND ?3 LIKE '%' || target_tag || '%')
                  OR (target_id IS NULL AND target_tag IS NULL))",
        )
        .bind(&[now.into(), target_id.into(), tags.unwrap_or("").into()])?
        .first::<CountRow>(None)
        .await?;

    Ok(result.map(|r| r.cnt > 0).unwrap_or(false))
}

async fn get_by_id(db: &D1Database, id: &str) -> Result<MaintenanceWindow> {
    db.prepare("SELECT * FROM maintenance_windows WHERE id = ?1")
        .bind(&[id.into()])?
        .first::<MaintenanceWindow>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("Maintenance window not found: {}", id)))
}

#[derive(Deserialize)]
struct CountRow {
    cnt: i64,
}
