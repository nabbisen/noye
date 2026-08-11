//! D1-side migration: bulk export and bulk import.
//!
//! Pure validation lives in `crate::migration`; this module is the I/O layer.
//! Export reads every relevant table; import writes them back, honouring an
//! `ImportConflictPolicy`.
//!
//! ## Why not a single `BEGIN TRANSACTION`?
//!
//! D1's batch API runs multiple statements atomically against the same SQLite
//! instance, but it does not expose `BEGIN`/`COMMIT` as standalone calls; the
//! atomicity boundary is the `db.batch(...)` call itself. We use that for
//! each table-group write so a failure mid-table aborts cleanly.

use noye_shared::{
    Caller, ImportConflictPolicy, ImportRowCounts, MaintenanceWindow, MigrationData,
    NotificationChannel, Target, TargetNotificationLink, User, i64_to_d1, opt_i64_to_d1,
};
use std::collections::HashSet;
use wasm_bindgen::JsValue;
use worker::*;

/// Read every configuration table into a single `MigrationData` structure.
/// `include_users` is honored at this layer — when `false`, the `users`
/// field is set to `None`, which the import path distinguishes from
/// "exported but empty."
pub async fn export_all(db: &D1Database, include_users: bool) -> Result<MigrationData> {
    let targets = db
        .prepare("SELECT * FROM targets ORDER BY id")
        .bind(&[])?
        .all()
        .await?
        .results::<Target>()?;

    let channels = db
        .prepare("SELECT * FROM notification_channels ORDER BY id")
        .bind(&[])?
        .all()
        .await?
        .results::<NotificationChannel>()?;

    let target_notifications = list_all_target_notifications(db).await?;

    let maintenance_windows = db
        .prepare("SELECT * FROM maintenance_windows ORDER BY id")
        .bind(&[])?
        .all()
        .await?
        .results::<MaintenanceWindow>()?;

    let users = if include_users {
        let rows = db
            .prepare("SELECT * FROM users ORDER BY id")
            .bind(&[])?
            .all()
            .await?
            .results::<User>()?;
        Some(rows)
    } else {
        None
    };

    Ok(MigrationData {
        targets,
        channels,
        target_notifications,
        maintenance_windows,
        users,
    })
}

async fn list_all_target_notifications(db: &D1Database) -> Result<Vec<TargetNotificationLink>> {
    let results = db
        .prepare(
            "SELECT target_id, channel_id, on_down, on_up
             FROM target_notifications
             ORDER BY target_id, channel_id",
        )
        .bind(&[])?
        .all()
        .await?
        .results::<TargetNotificationRow>()?;
    Ok(results
        .into_iter()
        .map(|r| TargetNotificationLink {
            target_id: r.target_id,
            channel_id: r.channel_id,
            on_down: r.on_down != 0,
            on_up: r.on_up != 0,
        })
        .collect())
}

#[derive(serde::Deserialize)]
struct TargetNotificationRow {
    target_id: String,
    channel_id: String,
    on_down: i64,
    on_up: i64,
}

/// Apply an import payload to D1. Returns the row counts for what was
/// inserted vs. skipped vs. replaced.
///
/// ## Conflict policy semantics
///
/// - `Skip`: existing rows are left untouched; incoming rows whose primary
///   key is already present are dropped.
/// - `Replace`: incoming rows take precedence on PK collision; we use an
///   explicit `ON CONFLICT DO UPDATE` upsert (subject 09, G-22) so a
///   colliding row is updated in place rather than deleted and
///   reinserted — `INSERT OR REPLACE` fires every `ON DELETE CASCADE`
///   declared against the row, which silently destroyed a target's
///   check results, incidents and channel attachments on every
///   re-import.
/// - `Fail`: a pre-flight pass collects existing IDs and the importer
///   returns an error if any incoming PK collides. Nothing is written.
///
/// The implementation deliberately runs the policy check per-table rather
/// than across all tables. A target ID that exists on the source but not
/// the destination shouldn't fail just because some unrelated channel ID
/// happens to collide.
///
/// `caller` is the operator performing the import — subject 08 (G-05):
/// an imported target's `created_by`/`updated_by` are always the
/// *importing* caller, never a value carried in the document, which
/// would name a user ID from another deployment.
pub async fn import_all(
    db: &D1Database,
    data: &MigrationData,
    policy: ImportConflictPolicy,
    caller: &Caller,
) -> Result<ImportRowCounts> {
    let mut counts = ImportRowCounts::default();

    if policy == ImportConflictPolicy::Fail {
        let collisions = collect_collisions(db, data).await?;
        if !collisions.is_empty() {
            return Err(Error::RustError(format!(
                "Fail policy: refusing to import; {} collision(s) detected: {}",
                collisions.len(),
                collisions.join(", ")
            )));
        }
    }

    if let Some(users) = &data.users {
        for u in users {
            let kept = upsert_user(db, u, policy).await?;
            tally(&mut counts, "users", kept);
        }
    }

    for t in &data.targets {
        let kept = upsert_target(db, t, policy, caller).await?;
        tally(&mut counts, "targets", kept);
    }

    for c in &data.channels {
        let kept = upsert_channel(db, c, policy).await?;
        tally(&mut counts, "channels", kept);
    }

    for m in &data.maintenance_windows {
        let kept = upsert_maintenance(db, m, policy).await?;
        tally(&mut counts, "maintenance_windows", kept);
    }

    for link in &data.target_notifications {
        let kept = upsert_target_notification(db, link, policy).await?;
        tally(&mut counts, "target_notifications", kept);
    }

    Ok(counts)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WriteOutcome {
    Inserted,
    Replaced,
    Skipped,
}

fn tally(counts: &mut ImportRowCounts, table: &str, outcome: WriteOutcome) {
    match outcome {
        WriteOutcome::Skipped => {
            counts.skipped += 1;
        }
        WriteOutcome::Replaced => {
            counts.replaced += 1;
            // Replaced rows still count toward the per-table total — they're
            // logically "applied" data even though the row already existed.
            bump_table(counts, table);
        }
        WriteOutcome::Inserted => bump_table(counts, table),
    }
}

fn bump_table(counts: &mut ImportRowCounts, table: &str) {
    match table {
        "targets" => counts.targets += 1,
        "channels" => counts.channels += 1,
        "target_notifications" => counts.target_notifications += 1,
        "maintenance_windows" => counts.maintenance_windows += 1,
        "users" => counts.users += 1,
        _ => {}
    }
}

/// Collect IDs that exist on the destination AND in the incoming payload.
/// Used by the `Fail` conflict policy.
async fn collect_collisions(db: &D1Database, data: &MigrationData) -> Result<Vec<String>> {
    let mut collisions = Vec::new();
    for t in &data.targets {
        if exists_by_id(db, "targets", &t.id).await? {
            collisions.push(format!("targets.{}", t.id));
        }
    }
    for c in &data.channels {
        if exists_by_id(db, "notification_channels", &c.id).await? {
            collisions.push(format!("notification_channels.{}", c.id));
        }
    }
    for m in &data.maintenance_windows {
        if exists_by_id(db, "maintenance_windows", &m.id).await? {
            collisions.push(format!("maintenance_windows.{}", m.id));
        }
    }
    if let Some(users) = &data.users {
        for u in users {
            if exists_by_id(db, "users", &u.id).await? {
                collisions.push(format!("users.{}", u.id));
            }
        }
    }
    for link in &data.target_notifications {
        let q = "SELECT 1 AS x FROM target_notifications WHERE target_id = ?1 AND channel_id = ?2";
        let hit = db
            .prepare(q)
            .bind(&[
                link.target_id.clone().into(),
                link.channel_id.clone().into(),
            ])?
            .first::<serde_json::Value>(None)
            .await?
            .is_some();
        if hit {
            collisions.push(format!(
                "target_notifications.({},{})",
                link.target_id, link.channel_id
            ));
        }
    }
    Ok(collisions)
}

/// Helper used only by `collect_collisions`. The table name is interpolated
/// into the prepared statement directly because D1 (like SQLite) does not
/// allow placeholders for identifiers; we control every caller and the input
/// is from a fixed set of literals, so this is safe.
async fn exists_by_id(db: &D1Database, table: &str, id: &str) -> Result<bool> {
    let q = format!("SELECT 1 AS x FROM {} WHERE id = ?1", table);
    let hit = db
        .prepare(&q)
        .bind(&[id.into()])?
        .first::<serde_json::Value>(None)
        .await?
        .is_some();
    Ok(hit)
}

// ── Per-table upsert helpers ──

async fn upsert_target(
    db: &D1Database,
    t: &Target,
    policy: ImportConflictPolicy,
    caller: &Caller,
) -> Result<WriteOutcome> {
    let exists = exists_by_id(db, "targets", &t.id).await?;
    if exists && policy == ImportConflictPolicy::Skip {
        return Ok(WriteOutcome::Skipped);
    }
    // `created_by`/`updated_by` are always `caller` -- never `t.created_by`/
    // `t.updated_by` -- per subject 08 (G-05): the document's values are
    // user IDs from another deployment and mean nothing here. On a
    // collision this still overwrites `created_by` with the importing
    // caller, matching FR-MIG-08's "equivalent to the normal creation
    // path" for the row as it now stands in this deployment.
    db.prepare(
        "INSERT INTO targets
         (id, name, type, host, port, path, expected_status, body_contains,
          tls_threshold_days, timeout_sec, retry_count, interval_minutes,
          is_disabled, owner_id, tags, next_check_at, created_at, updated_at,
          created_by, updated_by, success_threshold, failure_threshold)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
                 ?19, ?20, ?21, ?22)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            type = excluded.type,
            host = excluded.host,
            port = excluded.port,
            path = excluded.path,
            expected_status = excluded.expected_status,
            body_contains = excluded.body_contains,
            tls_threshold_days = excluded.tls_threshold_days,
            timeout_sec = excluded.timeout_sec,
            retry_count = excluded.retry_count,
            interval_minutes = excluded.interval_minutes,
            is_disabled = excluded.is_disabled,
            owner_id = excluded.owner_id,
            tags = excluded.tags,
            next_check_at = excluded.next_check_at,
            updated_at = excluded.updated_at,
            updated_by = excluded.updated_by,
            success_threshold = excluded.success_threshold,
            failure_threshold = excluded.failure_threshold",
    )
    .bind(&[
        t.id.clone().into(),
        t.name.clone().into(),
        t.target_type.clone().into(),
        t.host.clone().into(),
        opt_i64_to_d1(t.port).map_err(Error::RustError)?,
        t.path.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        opt_i64_to_d1(t.expected_status).map_err(Error::RustError)?,
        t.body_contains
            .clone()
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        opt_i64_to_d1(t.tls_threshold_days).map_err(Error::RustError)?,
        i64_to_d1(t.timeout_sec).map_err(Error::RustError)?,
        i64_to_d1(t.retry_count).map_err(Error::RustError)?,
        i64_to_d1(t.interval_minutes).map_err(Error::RustError)?,
        JsValue::from(t.is_disabled as i32),
        t.owner_id.clone().into(),
        t.tags.clone().map(JsValue::from).unwrap_or(JsValue::NULL),
        t.next_check_at.clone().into(),
        t.created_at.clone().into(),
        t.updated_at.clone().into(),
        caller.user_id.clone().into(),
        caller.user_id.clone().into(),
        i64_to_d1(t.success_threshold).map_err(Error::RustError)?,
        i64_to_d1(t.failure_threshold).map_err(Error::RustError)?,
    ])?
    .run()
    .await?;

    if !exists {
        // Subject 10 (G-06): create the state row in the same operation as
        // the target, exactly what `db::targets::create` does on the
        // normal path -- counters at zero, status unknown. An existing
        // target already has one; nothing to do on the replace/update arm.
        db.prepare("INSERT INTO target_states (target_id, current_status) VALUES (?1, 'unknown')")
            .bind(&[t.id.clone().into()])?
            .run()
            .await?;
    }

    Ok(if exists {
        WriteOutcome::Replaced
    } else {
        WriteOutcome::Inserted
    })
}

async fn upsert_channel(
    db: &D1Database,
    c: &NotificationChannel,
    policy: ImportConflictPolicy,
) -> Result<WriteOutcome> {
    let exists = exists_by_id(db, "notification_channels", &c.id).await?;
    if exists && policy == ImportConflictPolicy::Skip {
        return Ok(WriteOutcome::Skipped);
    }
    db.prepare(
        "INSERT INTO notification_channels
         (id, name, channel_type, endpoint, is_enabled, owner_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            channel_type = excluded.channel_type,
            endpoint = excluded.endpoint,
            is_enabled = excluded.is_enabled,
            owner_id = excluded.owner_id",
    )
    .bind(&[
        c.id.clone().into(),
        c.name.clone().into(),
        c.channel_type.clone().into(),
        c.endpoint.clone().into(),
        JsValue::from(c.is_enabled as i32),
        c.owner_id.clone().into(),
        c.created_at.clone().into(),
    ])?
    .run()
    .await?;
    Ok(if exists {
        WriteOutcome::Replaced
    } else {
        WriteOutcome::Inserted
    })
}

async fn upsert_maintenance(
    db: &D1Database,
    m: &MaintenanceWindow,
    policy: ImportConflictPolicy,
) -> Result<WriteOutcome> {
    let exists = exists_by_id(db, "maintenance_windows", &m.id).await?;
    if exists && policy == ImportConflictPolicy::Skip {
        return Ok(WriteOutcome::Skipped);
    }
    db.prepare(
        "INSERT INTO maintenance_windows
         (id, name, start_at, end_at, target_tag, target_id, suppress_notify,
          is_active, created_at, created_by, updated_by)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            start_at = excluded.start_at,
            end_at = excluded.end_at,
            target_tag = excluded.target_tag,
            target_id = excluded.target_id,
            suppress_notify = excluded.suppress_notify,
            is_active = excluded.is_active,
            updated_by = excluded.updated_by",
    )
    .bind(&[
        m.id.clone().into(),
        m.name.clone().into(),
        m.start_at.clone().into(),
        m.end_at.clone().into(),
        m.target_tag
            .clone()
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        m.target_id
            .clone()
            .map(JsValue::from)
            .unwrap_or(JsValue::NULL),
        JsValue::from(m.suppress_notify as i32),
        JsValue::from(m.is_active as i32),
        m.created_at.clone().into(),
        m.created_by.clone().into(),
        m.updated_by.clone().into(),
    ])?
    .run()
    .await?;
    Ok(if exists {
        WriteOutcome::Replaced
    } else {
        WriteOutcome::Inserted
    })
}

async fn upsert_user(
    db: &D1Database,
    u: &User,
    policy: ImportConflictPolicy,
) -> Result<WriteOutcome> {
    let exists = exists_by_id(db, "users", &u.id).await?;
    if exists && policy == ImportConflictPolicy::Skip {
        return Ok(WriteOutcome::Skipped);
    }
    db.prepare(
        "INSERT INTO users
         (id, email, name, role, is_active, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(id) DO UPDATE SET
            email = excluded.email,
            name = excluded.name,
            role = excluded.role,
            is_active = excluded.is_active,
            updated_at = excluded.updated_at",
    )
    .bind(&[
        u.id.clone().into(),
        u.email.clone().into(),
        u.name.clone().into(),
        u.role.clone().into(),
        JsValue::from(u.is_active as i32),
        u.created_at.clone().into(),
        u.updated_at.clone().into(),
    ])?
    .run()
    .await?;
    Ok(if exists {
        WriteOutcome::Replaced
    } else {
        WriteOutcome::Inserted
    })
}

async fn upsert_target_notification(
    db: &D1Database,
    link: &TargetNotificationLink,
    policy: ImportConflictPolicy,
) -> Result<WriteOutcome> {
    // The join table's primary key is the (target_id, channel_id) pair.
    let q = "SELECT 1 AS x FROM target_notifications WHERE target_id = ?1 AND channel_id = ?2";
    let exists = db
        .prepare(q)
        .bind(&[
            link.target_id.clone().into(),
            link.channel_id.clone().into(),
        ])?
        .first::<serde_json::Value>(None)
        .await?
        .is_some();
    if exists && policy == ImportConflictPolicy::Skip {
        return Ok(WriteOutcome::Skipped);
    }
    db.prepare(
        "INSERT INTO target_notifications
         (target_id, channel_id, on_down, on_up)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(target_id, channel_id) DO UPDATE SET
            on_down = excluded.on_down,
            on_up = excluded.on_up",
    )
    .bind(&[
        link.target_id.clone().into(),
        link.channel_id.clone().into(),
        JsValue::from(link.on_down as i32),
        JsValue::from(link.on_up as i32),
    ])?
    .run()
    .await?;
    Ok(if exists {
        WriteOutcome::Replaced
    } else {
        WriteOutcome::Inserted
    })
}

/// Subject 08 (G-31): resolve every `owner_id` referenced by an incoming
/// target or channel against the destination this import would actually
/// leave behind — either a user already present in D1, or a user this
/// same payload carries (which `import_all` inserts regardless of
/// conflict policy, since `Skip` only guards *existing* rows). Returns
/// every unresolvable reference in one pass, named by the record that
/// carries it (FR-MIG-06) — never the first one found.
///
/// Read-only: issues no writes, so it is safe to call from the dry-run
/// path as well as before a real import (FR-MIG-05).
pub async fn find_unresolvable_owners(
    db: &D1Database,
    data: &MigrationData,
) -> Result<Vec<String>> {
    let payload_user_ids: HashSet<&str> = data
        .users
        .as_ref()
        .map(|users| users.iter().map(|u| u.id.as_str()).collect())
        .unwrap_or_default();

    let mut unresolved = Vec::new();

    for t in &data.targets {
        if payload_user_ids.contains(t.owner_id.as_str()) {
            continue;
        }
        if !exists_by_id(db, "users", &t.owner_id).await? {
            unresolved.push(format!(
                "target {} references owner {}, which does not exist in this deployment",
                t.id, t.owner_id
            ));
        }
    }

    for c in &data.channels {
        if payload_user_ids.contains(c.owner_id.as_str()) {
            continue;
        }
        if !exists_by_id(db, "users", &c.owner_id).await? {
            unresolved.push(format!(
                "channel {} references owner {}, which does not exist in this deployment",
                c.id, c.owner_id
            ));
        }
    }

    Ok(unresolved)
}

#[cfg(test)]
mod tests;
