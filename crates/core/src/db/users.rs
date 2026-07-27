use noye_shared::{ManageUserInput, User};
use wasm_bindgen::JsValue;
use worker::*;

pub async fn list_all(db: &D1Database) -> Result<Vec<User>> {
    let results = db.prepare("SELECT * FROM users ORDER BY name").bind(&[])?.all().await?;
    results.results::<User>()
}

pub async fn get_by_email(db: &D1Database, email: &str) -> Result<Option<User>> {
    db.prepare("SELECT * FROM users WHERE email = ?1")
        .bind(&[email.into()])?
        .first::<User>(None)
        .await
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
