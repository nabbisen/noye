use noye_shared::{Caller, Incident};
use wasm_bindgen::JsValue;
use worker::*;

pub async fn open(db: &D1Database, target_id: &str, cause: &str) -> Result<Incident> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO incidents (id, target_id, status, opened_at, cause, created_by)
         VALUES (?1, ?2, 'open', ?3, ?4, 'system')",
    )
    .bind(&[id.clone().into(), target_id.into(), now.into(), cause.into()])?
    .run()
    .await?;

    get_by_id(db, &id).await
}

pub async fn resolve(db: &D1Database, id: &str, note: Option<&str>, caller: &Caller) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let incident = get_by_id(db, id).await?;
    let duration = calculate_duration(&incident.opened_at, &now);

    db.prepare(
        "UPDATE incidents SET status = 'resolved', resolved_at = ?1,
         duration_sec = ?2, resolution_note = ?3, created_by = ?4 WHERE id = ?5",
    )
    .bind(&[
        now.into(),
        JsValue::from(duration),
        note.map(JsValue::from).unwrap_or(JsValue::NULL),
        caller.user_id.clone().into(),
        id.into(),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn auto_resolve(db: &D1Database, target_id: &str) -> Result<()> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    db.prepare(
        "UPDATE incidents SET status = 'resolved', resolved_at = ?1, created_by = 'system'
         WHERE target_id = ?2 AND status = 'open'",
    )
    .bind(&[now.into(), target_id.into()])?
    .run()
    .await?;
    Ok(())
}

pub async fn list_recent(db: &D1Database, limit: i64) -> Result<Vec<Incident>> {
    let results = db
        .prepare("SELECT * FROM incidents ORDER BY opened_at DESC LIMIT ?1")
        .bind(&[JsValue::from(limit)])?
        .all()
        .await?;
    results.results::<Incident>()
}

#[allow(dead_code)]
pub async fn get_open_for_target(db: &D1Database, target_id: &str) -> Result<Option<Incident>> {
    db.prepare("SELECT * FROM incidents WHERE target_id = ?1 AND status = 'open' LIMIT 1")
        .bind(&[target_id.into()])?
        .first::<Incident>(None)
        .await
}

async fn get_by_id(db: &D1Database, id: &str) -> Result<Incident> {
    db.prepare("SELECT * FROM incidents WHERE id = ?1")
        .bind(&[id.into()])?
        .first::<Incident>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("Incident not found: {}", id)))
}

fn calculate_duration(start: &str, end: &str) -> i64 {
    let start_dt = chrono::NaiveDateTime::parse_from_str(start, "%Y-%m-%dT%H:%M:%SZ");
    let end_dt = chrono::NaiveDateTime::parse_from_str(end, "%Y-%m-%dT%H:%M:%SZ");
    match (start_dt, end_dt) {
        (Ok(s), Ok(e)) => (e - s).num_seconds(),
        _ => 0,
    }
}
