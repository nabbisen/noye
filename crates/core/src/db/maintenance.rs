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
          exclude_from_sla, is_active, created_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 1, ?9, ?10, ?11)",
    )
    .bind(&[
        id.clone().into(),
        input.name.clone().into(),
        input.start_at.clone().into(),
        input.end_at.clone().into(),
        input
            .target_tag
            .clone()
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        input
            .target_id
            .clone()
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        JsValue::from(input.suppress_notify.unwrap_or(true) as i32),
        JsValue::from(input.exclude_from_sla.unwrap_or(true) as i32),
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

/// Applicability rule shared by `list_in_window` and `is_under_maintenance`:
/// direct match by `target_id`, or an *exact* tag match against the
/// `target_tags` relation (subject 12, G-09/G-27 -- no more substring
/// matching, and no more stored-tag-as-pattern wildcard leakage), or
/// global (both `target_id` and `target_tag` NULL).
/// `?1` is always the target being checked; callers number their other
/// placeholders around it.
const APPLICABILITY_CLAUSE: &str = "(target_id = ?1
     OR (target_tag IS NOT NULL AND EXISTS (
           SELECT 1 FROM target_tags tt
           WHERE tt.target_id = ?1 AND tt.tag = target_tag))
     OR (target_id IS NULL AND target_tag IS NULL))";

/// List maintenance windows applicable to the given target whose
/// `[start_at, end_at)` overlaps the report window, and whose flags mean
/// they actually affect the SLA figure (subject 11, G-07): `is_active`
/// and `exclude_from_sla` must both be set, matching `is_under_maintenance`'s
/// own flag discipline below.
pub async fn list_in_window(
    db: &D1Database,
    target_id: &str,
    window_start: &str,
    window_end: &str,
) -> Result<Vec<MaintenanceWindow>> {
    let results = db
        .prepare(format!(
            "SELECT * FROM maintenance_windows
             WHERE is_active = 1 AND exclude_from_sla = 1
               AND start_at < ?3
               AND end_at > ?2
               AND {APPLICABILITY_CLAUSE}
             ORDER BY start_at"
        ))
        .bind(&[target_id.into(), window_start.into(), window_end.into()])?
        .all()
        .await?;
    results.results::<MaintenanceWindow>()
}

/// Subject 11 (G-07): both `is_active` and `suppress_notify` must be set
/// for a window to actually silence notifications -- previously only
/// `is_active` was checked, so a window explicitly marked
/// non-suppressing still suppressed.
pub async fn is_under_maintenance(db: &D1Database, target_id: &str) -> Result<bool> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let result = db
        .prepare(format!(
            "SELECT COUNT(*) as cnt FROM maintenance_windows
             WHERE is_active = 1 AND suppress_notify = 1
               AND start_at <= ?2 AND end_at >= ?2
               AND {APPLICABILITY_CLAUSE}"
        ))
        .bind(&[target_id.into(), now.into()])?
        .first::<CountRow>(None)
        .await?;

    Ok(result.map(|r| r.cnt > 0).unwrap_or(false))
}

async fn get_by_id(db: &D1Database, id: &str) -> Result<MaintenanceWindow> {
    db.prepare("SELECT * FROM maintenance_windows WHERE id = ?1")
        .bind(&[id.into()])?
        .first::<MaintenanceWindow>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("maintenance window not found: {}", id)))
}

#[derive(Deserialize)]
struct CountRow {
    cnt: i64,
}

#[cfg(test)]
mod tests;
