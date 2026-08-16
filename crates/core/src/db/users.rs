use noye_shared::{ManageUserInput, User};
use wasm_bindgen::JsValue;
use worker::*;

pub async fn list_all(db: &D1Database) -> Result<Vec<User>> {
    let results = db
        .prepare("SELECT * FROM users ORDER BY name")
        .bind(&[])?
        .all()
        .await?;
    results.results::<User>()
}

pub async fn get_by_email(db: &D1Database, email: &str) -> Result<Option<User>> {
    db.prepare("SELECT * FROM users WHERE email = ?1")
        .bind(&[email.into()])?
        .first::<User>(None)
        .await
}

pub async fn get_by_sub(db: &D1Database, sub: &str) -> Result<Option<User>> {
    db.prepare("SELECT * FROM users WHERE sub = ?1")
        .bind(&[sub.into()])?
        .first::<User>(None)
        .await
}

/// Resolve a caller's identity for login (subject 19, G-16): `sub`
/// first, falling back to `email` exactly once to backfill an
/// existing pre-`sub` row, then storing `sub` for every subsequent
/// login.
///
/// This is an authentication path. The fallback matches on email
/// *and* only when the stored row's `sub` is still `NULL` -- never a
/// row whose `sub` is already claimed by a different subject. An
/// unconstrained email fallback would let an unknown subject match an
/// existing row: a straightforward authentication bypass (T-98).
pub async fn resolve_by_identity(db: &D1Database, sub: &str, email: &str) -> Result<Option<User>> {
    if let Some(user) = get_by_sub(db, sub).await? {
        return Ok(Some(user));
    }

    let Some(candidate) = get_by_email(db, email).await? else {
        return Ok(None);
    };
    if candidate.sub.is_some() {
        // This email belongs to a row already claimed by a different
        // subject -- refuse rather than match the wrong identity.
        return Ok(None);
    }

    // Backfill, guarded by `AND sub IS NULL` so a concurrent first
    // login for the same person can't overwrite whichever request
    // wins the race (pre-flight .git-exclude/reviewed/067-m2d-
    // preflight.md §7). If we lose it, re-resolving by sub picks up
    // the winner instead of surfacing a constraint error to a
    // legitimate user -- do not relax the `sub` UNIQUE constraint;
    // it's what makes this race safe rather than silently wrong.
    db.prepare("UPDATE users SET sub = ?1 WHERE id = ?2 AND sub IS NULL")
        .bind(&[sub.into(), candidate.id.clone().into()])?
        .run()
        .await?;

    match get_by_sub(db, sub).await? {
        Some(user) => Ok(Some(user)),
        None => Ok(Some(candidate)),
    }
}

pub async fn upsert(db: &D1Database, input: &ManageUserInput) -> Result<User> {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let existing = get_by_email(db, &input.email).await?;

    match existing {
        Some(user) => {
            db.prepare(
                "UPDATE users SET name = ?1, role = ?2, is_active = ?3, updated_at = ?4 WHERE id = ?5",
            )
            .bind(&[
                input.name.clone().into(),
                input.role.clone().into(),
                JsValue::from(input.is_active.unwrap_or(true) as i32),
                now.into(),
                user.id.clone().into(),
            ])?
            .run()
            .await?;

            get_by_email(db, &input.email)
                .await?
                .ok_or_else(|| Error::RustError("User disappeared after update".to_string()))
        }
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            db.prepare(
                "INSERT INTO users (id, email, name, role, is_active, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )
            .bind(&[
                id.into(),
                input.email.clone().into(),
                input.name.clone().into(),
                input.role.clone().into(),
                JsValue::from(input.is_active.unwrap_or(true) as i32),
                now.clone().into(),
                now.into(),
            ])?
            .run()
            .await?;

            get_by_email(db, &input.email)
                .await?
                .ok_or_else(|| Error::RustError("User not found after insert".to_string()))
        }
    }
}
