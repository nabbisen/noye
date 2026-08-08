use noye_shared::{Caller, Incident, i64_to_d1};
use wasm_bindgen::JsValue;
use worker::*;

pub async fn open(db: &D1Database, target_id: &str, cause: &str) -> Result<Incident> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO incidents (id, target_id, status, opened_at, cause, created_by)
         VALUES (?1, ?2, 'open', ?3, ?4, 'system')",
    )
    .bind(&[
        id.clone().into(),
        target_id.into(),
        now.into(),
        cause.into(),
    ])?
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
        i64_to_d1(duration).map_err(Error::RustError)?,
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
        .bind(&[i64_to_d1(limit).map_err(Error::RustError)?])?
        .all()
        .await?;
    results.results::<Incident>()
}

/// List incidents on a given target whose `[opened_at, resolved_at)` overlaps
/// the given window. Open incidents (NULL `resolved_at`) match if they were
/// opened before `window_end`. Used by the SLA report.
pub async fn list_in_window(
    db: &D1Database,
    target_id: &str,
    window_start: &str,
    window_end: &str,
) -> Result<Vec<Incident>> {
    let results = db
        .prepare(
            "SELECT * FROM incidents
             WHERE target_id = ?1
               AND opened_at < ?3
               AND (resolved_at IS NULL OR resolved_at > ?2)
             ORDER BY opened_at",
        )
        .bind(&[target_id.into(), window_start.into(), window_end.into()])?
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

#[cfg(test)]
mod tests {
    use super::calculate_duration;

    #[test]
    fn elapsed_seconds_within_one_minute() {
        let start = "2026-01-01T00:00:00Z";
        let end = "2026-01-01T00:00:30Z";
        assert_eq!(calculate_duration(start, end), 30);
    }

    #[test]
    fn elapsed_seconds_across_hour_boundary() {
        let start = "2026-01-01T23:59:30Z";
        let end = "2026-01-02T00:00:30Z";
        assert_eq!(calculate_duration(start, end), 60);
    }

    #[test]
    fn elapsed_seconds_zero_when_same_instant() {
        let stamp = "2026-01-01T12:34:56Z";
        assert_eq!(calculate_duration(stamp, stamp), 0);
    }

    #[test]
    fn negative_duration_when_end_before_start() {
        // chrono allows negative durations; calculate_duration returns the raw difference.
        let start = "2026-01-01T00:01:00Z";
        let end = "2026-01-01T00:00:00Z";
        assert_eq!(calculate_duration(start, end), -60);
    }

    #[test]
    fn invalid_format_returns_zero() {
        // The contract: any unparseable timestamp yields 0 (defensive default).
        assert_eq!(calculate_duration("not-a-date", "2026-01-01T00:00:00Z"), 0);
        assert_eq!(calculate_duration("2026-01-01T00:00:00Z", "not-a-date"), 0);
        assert_eq!(calculate_duration("foo", "bar"), 0);
        assert_eq!(calculate_duration("", ""), 0);
    }

    #[test]
    fn long_duration_in_seconds() {
        // 24 hours
        let start = "2026-01-01T00:00:00Z";
        let end = "2026-01-02T00:00:00Z";
        assert_eq!(calculate_duration(start, end), 86_400);
    }
}
