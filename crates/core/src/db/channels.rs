use noye_shared::{
    AttachChannelInput, AttachedChannel, AttachedTarget, Caller, CreateNotificationChannelInput,
    NotificationChannel, UpdateNotificationChannelInput,
};
use serde::Deserialize;
use wasm_bindgen::JsValue;
use worker::*;

// ─────────────────────────────────────────────
//  Validation (pure helpers, unit-tested below)
// ─────────────────────────────────────────────

/// Allowed values for the `channel_type` column.
pub const CHANNEL_TYPES: &[&str] = &["webhook", "email", "slack"];

/// Validate the inputs of a create-or-update operation.
///
/// Pure helper so the rules are exercisable from unit tests without a
/// D1 binding. Errors are returned as static `&str` so the caller can wrap
/// them into a `worker::Error::RustError` or render them as form feedback.
pub fn validate_channel_inputs(
    channel_type: &str,
    endpoint: &str,
    name: &str,
) -> std::result::Result<(), &'static str> {
    if name.trim().is_empty() {
        return Err("name must not be empty");
    }
    if !CHANNEL_TYPES.contains(&channel_type) {
        return Err("channel_type must be one of: webhook, email, slack");
    }
    validate_endpoint(channel_type, endpoint)
}

/// Validate that `endpoint` looks well-formed for the given channel type.
///
/// - `webhook` and `slack`: must start with `https://` (Cloudflare Workers
///   `fetch` only allows TLS for outbound) and have a non-empty host
/// - `email`: must contain exactly one `@` with non-empty local-part and
///   domain, and at least one dot in the domain
pub fn validate_endpoint(
    channel_type: &str,
    endpoint: &str,
) -> std::result::Result<(), &'static str> {
    let trimmed = endpoint.trim();
    if trimmed.is_empty() {
        return Err("endpoint must not be empty");
    }

    match channel_type {
        "webhook" | "slack" => {
            if !trimmed.starts_with("https://") {
                return Err("endpoint must start with https://");
            }
            // Reject "https://" with no host
            let after_scheme = &trimmed["https://".len()..];
            // Strip path so "https://x.com/path" is fine but "https:///path" is not
            let host = after_scheme.split('/').next().unwrap_or("");
            if host.is_empty() {
                return Err("endpoint must include a host");
            }
            if host.contains(' ') {
                return Err("endpoint host must not contain whitespace");
            }
        }
        "email" => {
            // Bytes-only check; we don't need full RFC 5322 here.
            let parts: Vec<&str> = trimmed.split('@').collect();
            if parts.len() != 2 {
                return Err("email must contain exactly one '@'");
            }
            let (local, domain) = (parts[0], parts[1]);
            if local.is_empty() {
                return Err("email local-part must not be empty");
            }
            if domain.is_empty() {
                return Err("email domain must not be empty");
            }
            if !domain.contains('.') {
                return Err("email domain must contain a dot");
            }
            if trimmed.contains(' ') {
                return Err("email must not contain whitespace");
            }
        }
        _ => return Err("channel_type must be one of: webhook, email, slack"),
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  CRUD: notification_channels
// ─────────────────────────────────────────────

pub async fn list_channels(db: &D1Database, caller: &Caller) -> Result<Vec<NotificationChannel>> {
    let results = if caller.is_admin() {
        db.prepare("SELECT * FROM notification_channels ORDER BY name")
            .bind(&[])?
            .all()
            .await?
    } else {
        db.prepare("SELECT * FROM notification_channels WHERE owner_id = ?1 ORDER BY name")
            .bind(&[caller.user_id.clone().into()])?
            .all()
            .await?
    };
    let rows: Vec<ChannelRow> = results.results::<ChannelRow>()?;
    Ok(rows.into_iter().map(Into::into).collect())
}

pub async fn get_channel(db: &D1Database, id: &str) -> Result<NotificationChannel> {
    let row = db
        .prepare("SELECT * FROM notification_channels WHERE id = ?1")
        .bind(&[id.into()])?
        .first::<ChannelRow>(None)
        .await?
        .ok_or_else(|| Error::RustError(format!("Channel not found: {}", id)))?;
    Ok(row.into())
}

pub async fn create_channel(
    db: &D1Database,
    input: &CreateNotificationChannelInput,
    caller: &Caller,
) -> Result<NotificationChannel> {
    validate_channel_inputs(&input.channel_type, &input.endpoint, &input.name)
        .map_err(|e| Error::RustError(e.to_string()))?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    db.prepare(
        "INSERT INTO notification_channels
         (id, name, channel_type, endpoint, is_enabled, owner_id, created_at)
         VALUES (?1, ?2, ?3, ?4, 1, ?5, ?6)",
    )
    .bind(&[
        id.clone().into(),
        input.name.trim().into(),
        input.channel_type.clone().into(),
        input.endpoint.trim().into(),
        caller.user_id.clone().into(),
        now.into(),
    ])?
    .run()
    .await?;

    get_channel(db, &id).await
}

pub async fn update_channel(
    db: &D1Database,
    id: &str,
    input: &UpdateNotificationChannelInput,
) -> Result<NotificationChannel> {
    let current = get_channel(db, id).await?;

    let new_name = input
        .name
        .as_deref()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| current.name.clone());
    let new_endpoint = input
        .endpoint
        .as_deref()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| current.endpoint.clone());
    let new_enabled = input.is_enabled.unwrap_or(current.is_enabled);

    // Re-validate on update so a bad endpoint cannot be smuggled in via PATCH.
    validate_channel_inputs(&current.channel_type, &new_endpoint, &new_name)
        .map_err(|e| Error::RustError(e.to_string()))?;

    db.prepare(
        "UPDATE notification_channels
         SET name = ?1, endpoint = ?2, is_enabled = ?3
         WHERE id = ?4",
    )
    .bind(&[
        new_name.into(),
        new_endpoint.into(),
        JsValue::from(new_enabled as i32),
        id.into(),
    ])?
    .run()
    .await?;

    get_channel(db, id).await
}

pub async fn delete_channel(db: &D1Database, id: &str) -> Result<()> {
    db.prepare("DELETE FROM notification_channels WHERE id = ?1")
        .bind(&[id.into()])?
        .run()
        .await?;
    // ON DELETE CASCADE on target_notifications takes care of the join rows.
    Ok(())
}

// ─────────────────────────────────────────────
//  CRUD: target_notifications (join table)
// ─────────────────────────────────────────────

pub async fn list_attached_channels(
    db: &D1Database,
    target_id: &str,
) -> Result<Vec<AttachedChannel>> {
    let results = db
        .prepare(
            "SELECT nc.id AS channel_id, nc.name AS channel_name,
                    nc.channel_type, nc.endpoint, nc.is_enabled,
                    tn.on_down, tn.on_up
             FROM target_notifications tn
             JOIN notification_channels nc ON tn.channel_id = nc.id
             WHERE tn.target_id = ?1
             ORDER BY nc.name",
        )
        .bind(&[target_id.into()])?
        .all()
        .await?
        .results::<AttachedRow>()?;

    Ok(results
        .into_iter()
        .map(|r| AttachedChannel {
            channel_id: r.channel_id,
            channel_name: r.channel_name,
            channel_type: r.channel_type,
            endpoint: r.endpoint,
            is_enabled: r.is_enabled != 0,
            on_down: r.on_down != 0,
            on_up: r.on_up != 0,
        })
        .collect())
}

/// Reverse lookup: every target that a given channel is attached to, joined
/// with target metadata so the channel-detail page can show the impact zone.
pub async fn list_targets_for_channel(
    db: &D1Database,
    channel_id: &str,
) -> Result<Vec<AttachedTarget>> {
    let results = db
        .prepare(
            "SELECT t.id AS target_id, t.name AS target_name,
                    t.type AS target_type, t.host AS target_host,
                    tn.on_down, tn.on_up
             FROM target_notifications tn
             JOIN targets t ON tn.target_id = t.id
             WHERE tn.channel_id = ?1
             ORDER BY t.name",
        )
        .bind(&[channel_id.into()])?
        .all()
        .await?
        .results::<AttachedTargetRow>()?;

    Ok(results
        .into_iter()
        .map(|r| AttachedTarget {
            target_id: r.target_id,
            target_name: r.target_name,
            target_type: r.target_type,
            target_host: r.target_host,
            on_down: r.on_down != 0,
            on_up: r.on_up != 0,
        })
        .collect())
}

/// Idempotent attach: inserts or updates the trigger flags for a (target,
/// channel) pair.
pub async fn attach_channel(
    db: &D1Database,
    target_id: &str,
    input: &AttachChannelInput,
) -> Result<()> {
    // INSERT OR REPLACE keeps the call site simple. The composite primary key
    // (target_id, channel_id) ensures REPLACE updates the right row.
    db.prepare(
        "INSERT OR REPLACE INTO target_notifications
         (target_id, channel_id, on_down, on_up)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&[
        target_id.into(),
        input.channel_id.clone().into(),
        JsValue::from(input.on_down as i32),
        JsValue::from(input.on_up as i32),
    ])?
    .run()
    .await?;
    Ok(())
}

pub async fn detach_channel(db: &D1Database, target_id: &str, channel_id: &str) -> Result<()> {
    db.prepare("DELETE FROM target_notifications WHERE target_id = ?1 AND channel_id = ?2")
        .bind(&[target_id.into(), channel_id.into()])?
        .run()
        .await?;
    Ok(())
}

// ─────────────────────────────────────────────
//  Internal row types (decouple D1 column shape from the public type)
// ─────────────────────────────────────────────

#[derive(Deserialize)]
struct ChannelRow {
    id: String,
    name: String,
    channel_type: String,
    endpoint: String,
    is_enabled: i64,
    owner_id: String,
    created_at: String,
}

impl From<ChannelRow> for NotificationChannel {
    fn from(r: ChannelRow) -> Self {
        Self {
            id: r.id,
            name: r.name,
            channel_type: r.channel_type,
            endpoint: r.endpoint,
            is_enabled: r.is_enabled != 0,
            owner_id: r.owner_id,
            created_at: r.created_at,
        }
    }
}

#[derive(Deserialize)]
struct AttachedRow {
    channel_id: String,
    channel_name: String,
    channel_type: String,
    endpoint: String,
    is_enabled: i64,
    on_down: i64,
    on_up: i64,
}

#[derive(Deserialize)]
struct AttachedTargetRow {
    target_id: String,
    target_name: String,
    target_type: String,
    target_host: String,
    on_down: i64,
    on_up: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_endpoint_must_be_https() {
        assert!(validate_endpoint("webhook", "https://example.com/hook").is_ok());
        assert!(validate_endpoint("webhook", "http://example.com/hook").is_err());
        assert!(validate_endpoint("webhook", "ftp://example.com").is_err());
    }

    #[test]
    fn webhook_endpoint_requires_a_host() {
        assert!(validate_endpoint("webhook", "https://").is_err());
        assert!(validate_endpoint("webhook", "https:///path").is_err());
    }

    #[test]
    fn webhook_endpoint_rejects_whitespace_in_host() {
        assert!(validate_endpoint("webhook", "https://exam ple.com").is_err());
    }

    #[test]
    fn slack_endpoint_uses_same_rules_as_webhook() {
        assert!(validate_endpoint("slack", "https://hooks.slack.com/services/T/B/X").is_ok());
        assert!(validate_endpoint("slack", "http://hooks.slack.com/services/T/B/X").is_err());
    }

    #[test]
    fn email_endpoint_must_have_one_at_sign() {
        assert!(validate_endpoint("email", "alice@example.com").is_ok());
        assert!(validate_endpoint("email", "alice").is_err());
        assert!(validate_endpoint("email", "alice@@example.com").is_err());
        assert!(validate_endpoint("email", "@example.com").is_err());
        assert!(validate_endpoint("email", "alice@").is_err());
    }

    #[test]
    fn email_endpoint_must_have_dotted_domain() {
        assert!(validate_endpoint("email", "alice@localhost").is_err());
        assert!(validate_endpoint("email", "alice@example.com").is_ok());
        assert!(validate_endpoint("email", "alice@sub.example.co.jp").is_ok());
    }

    #[test]
    fn email_endpoint_rejects_whitespace() {
        assert!(validate_endpoint("email", "alice @example.com").is_err());
        assert!(validate_endpoint("email", "alice@exam ple.com").is_err());
    }

    #[test]
    fn unknown_channel_type_is_rejected() {
        assert!(validate_endpoint("smoke-signal", "https://example.com").is_err());
        assert!(validate_endpoint("", "alice@example.com").is_err());
    }

    #[test]
    fn validate_channel_inputs_rejects_empty_name() {
        let r = validate_channel_inputs("webhook", "https://example.com", "");
        assert!(r.is_err());
        let r = validate_channel_inputs("webhook", "https://example.com", "   ");
        assert!(r.is_err());
    }

    #[test]
    fn validate_channel_inputs_rejects_unknown_type() {
        let r = validate_channel_inputs("carrier-pigeon", "https://example.com", "name");
        assert!(r.is_err());
    }

    #[test]
    fn validate_channel_inputs_passes_valid_combinations() {
        assert!(validate_channel_inputs("webhook", "https://example.com/hook", "Prod").is_ok());
        assert!(validate_channel_inputs("email", "ops@example.com", "Ops list").is_ok());
        assert!(
            validate_channel_inputs("slack", "https://hooks.slack.com/services/X", "#alerts")
                .is_ok()
        );
    }

    #[test]
    fn channel_types_constant_matches_schema_check() {
        // The CHECK constraint in 0001_initial.sql is the source of truth.
        // If you add a new channel type, update both this test and the schema.
        assert_eq!(CHANNEL_TYPES, &["webhook", "email", "slack"]);
    }
}
