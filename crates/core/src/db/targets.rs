use noye_shared::{Caller, CreateTargetInput, StatusSummary, Target, UpdateTargetInput};
use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::*;

pub async fn get_status_summary(db: &D1Database) -> Result<StatusSummary> {
    let stmt = db.prepare(
        "SELECT
            COUNT(*) as total,
            SUM(CASE WHEN ts.current_status = 'up' THEN 1 ELSE 0 END) as up_count,
            SUM(CASE WHEN ts.current_status = 'down' THEN 1 ELSE 0 END) as down_count,
            SUM(CASE WHEN ts.current_status = 'degraded' THEN 1 ELSE 0 END) as degraded_count,
            SUM(CASE WHEN ts.current_status = 'maintenance' THEN 1 ELSE 0 END) as maint_count,
            SUM(CASE WHEN ts.current_status = 'unknown' OR ts.current_status IS NULL THEN 1 ELSE 0 END) as unknown_count,
            SUM(CASE WHEN t.is_disabled = 1 THEN 1 ELSE 0 END) as disabled_count
         FROM targets t
         LEFT JOIN target_states ts ON t.id = ts.target_id",
    );

    #[derive(Deserialize)]
    struct Row {
        total: Option<i64>,
        up_count: Option<i64>,
        down_count: Option<i64>,
        degraded_count: Option<i64>,
        maint_count: Option<i64>,
        unknown_count: Option<i64>,
        disabled_count: Option<i64>,
    }

    let row = stmt.first::<Row>(None).await?.unwrap_or(Row {
        total: Some(0),
        up_count: Some(0),
        down_count: Some(0),
        degraded_count: Some(0),
        maint_count: Some(0),
        unknown_count: Some(0),
        disabled_count: Some(0),
    });

    Ok(StatusSummary {
        total: row.total.unwrap_or(0),
        up: row.up_count.unwrap_or(0),
        down: row.down_count.unwrap_or(0),
        degraded: row.degraded_count.unwrap_or(0),
        maintenance: row.maint_count.unwrap_or(0),
        unknown: row.unknown_count.unwrap_or(0),
        disabled: row.disabled_count.unwrap_or(0),
    })
}

pub async fn list_all(db: &D1Database, caller: &Caller) -> Result<Vec<Target>> {
    let results = if caller.is_admin() {
        db.prepare("SELECT * FROM targets ORDER BY name")
            .bind(&[])?
            .all()
            .await?
    } else {
        db.prepare("SELECT * FROM targets WHERE owner_id = ?1 ORDER BY name")
            .bind(&[caller.user_id.clone().into()])?
            .all()
            .await?
    };
    results.results::<Target>()
}

pub async fn get_by_id(db: &D1Database, id: &str) -> Result<Target> {
    let stmt = db.prepare("SELECT * FROM targets WHERE id = ?1");
    stmt.bind(&[id.into()])?
        .first::<Target>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("Target not found: {}", id)))
}

pub async fn create(db: &D1Database, input: &CreateTargetInput, caller: &Caller) -> Result<Target> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO targets (id, name, type, host, port, path, expected_status, body_contains,
         tls_threshold_days, timeout_sec, retry_count, interval_minutes, owner_id, tags,
         next_check_at, created_at, updated_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)",
    )
    .bind(&[
        id.clone().into(), input.name.clone().into(), input.target_type.clone().into(),
        input.host.clone().into(),
        input.port.map(JsValue::from).unwrap_or(JsValue::NULL),
        input.path.clone().unwrap_or_else(|| "/".to_string()).into(),
        input.expected_status.map(JsValue::from).unwrap_or(JsValue::from(200)),
        input.body_contains.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        input.tls_threshold_days.map(JsValue::from).unwrap_or(JsValue::from(30)),
        JsValue::from(input.timeout_sec.unwrap_or(10)),
        JsValue::from(input.retry_count.unwrap_or(3)),
        JsValue::from(input.interval_minutes.unwrap_or(5)),
        caller.user_id.clone().into(),
        input.tags.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        now.clone().into(), now.clone().into(), now.clone().into(),
        caller.user_id.clone().into(), caller.user_id.clone().into(),
    ])?.run().await?;

    db.prepare("INSERT INTO target_states (target_id, current_status) VALUES (?1, 'unknown')")
        .bind(&[id.clone().into()])?
        .run()
        .await?;

    get_by_id(db, &id).await
}

pub async fn update(
    db: &D1Database,
    id: &str,
    input: &UpdateTargetInput,
    caller: &Caller,
) -> Result<Target> {
    let current = get_by_id(db, id).await?;
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "UPDATE targets SET name = ?1, host = ?2, port = ?3, path = ?4, expected_status = ?5,
         body_contains = ?6, tls_threshold_days = ?7, timeout_sec = ?8, retry_count = ?9,
         interval_minutes = ?10, is_disabled = ?11, tags = ?12, updated_at = ?13, updated_by = ?14
         WHERE id = ?15",
    )
    .bind(&[
        input.name.clone().unwrap_or(current.name).into(),
        input.host.clone().unwrap_or(current.host).into(),
        input
            .port
            .or(current.port)
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        input
            .path
            .clone()
            .or(current.path)
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        input
            .expected_status
            .or(current.expected_status)
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        input
            .body_contains
            .clone()
            .or(current.body_contains)
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        input
            .tls_threshold_days
            .or(current.tls_threshold_days)
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        JsValue::from(input.timeout_sec.unwrap_or(current.timeout_sec)),
        JsValue::from(input.retry_count.unwrap_or(current.retry_count)),
        JsValue::from(input.interval_minutes.unwrap_or(current.interval_minutes)),
        JsValue::from(input.is_disabled.unwrap_or(current.is_disabled) as i32),
        input
            .tags
            .clone()
            .or(current.tags)
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        now.into(),
        caller.user_id.clone().into(),
        id.into(),
    ])?
    .run()
    .await?;

    get_by_id(db, id).await
}

pub async fn delete(db: &D1Database, id: &str) -> Result<()> {
    db.prepare("DELETE FROM targets WHERE id = ?1")
        .bind(&[id.into()])?
        .run()
        .await?;
    Ok(())
}
