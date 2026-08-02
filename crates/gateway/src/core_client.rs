//! Service Binding client for the Core worker.
//!
//! The Gateway never touches D1 directly; all data operations go through the Core.
//! This module wraps the Core's internal REST API in a type-safe interface.
//!
//! Every request automatically carries the following headers:
//! - `X-Gateway-Token`: shared secret (validated by the Core)
//! - `X-Caller-*`: authenticated user info (when a Caller argument is supplied)

use noye_shared::{
    AttachChannelInput, AttachedChannel, AttachedTarget, AuditEntry, Caller, CheckResult,
    CreateMaintenanceInput, CreateNotificationChannelInput, CreateTargetInput, ImportRequest,
    ImportResult, Incident, LookupUserResult, MaintenanceWindow, ManageUserInput, MigrationExport,
    NotificationChannel, ResolveIncidentInput, SlaMultiReport, SlaReport, SlaSummary,
    StatusSummary, Target, TargetState, UpdateNotificationChannelInput, UpdateTargetInput, User,
    header,
};
use worker::*;

const CORE_BINDING: &str = "CORE";

/// Base URL used for internal HTTP calls to the Core.
/// Over a Service Binding the host portion of the URL is ignored, but worker-rs
/// still requires a syntactically valid URL, so we use `https://core.internal`.
const CORE_BASE_URL: &str = "https://core.internal";

/// Issue an HTTP call to the Core.
async fn call(
    env: &Env,
    method: Method,
    path: &str,
    caller: Option<&Caller>,
    body: Option<&serde_json::Value>,
) -> Result<Response> {
    let service = env.service(CORE_BINDING)?;

    let mut init = RequestInit::new();
    init.with_method(method);

    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("Accept", "application/json")?;

    // Forward the gateway token
    if let Ok(secret) = env.secret("GATEWAY_SHARED_TOKEN") {
        headers.set(header::GATEWAY_TOKEN, &secret.to_string())?;
    } else if let Ok(token) = env.var("GATEWAY_SHARED_TOKEN") {
        headers.set(header::GATEWAY_TOKEN, &token.to_string())?;
    }

    // Forward the caller information
    if let Some(c) = caller {
        headers.set(header::CALLER_USER_ID, &c.user_id)?;
        headers.set(header::CALLER_EMAIL, &c.email)?;
        headers.set(header::CALLER_NAME, &c.name)?;
        headers.set(header::CALLER_ROLE, &c.role)?;
    }

    init.with_headers(headers);

    if let Some(b) = body {
        let json = serde_json::to_string(b)
            .map_err(|e| Error::RustError(format!("body serialize error: {}", e)))?;
        init.with_body(Some(wasm_bindgen::JsValue::from_str(&json)));
    }

    let url = format!("{}{}", CORE_BASE_URL, path);
    let request = Request::new_with_init(&url, &init)?;
    service.fetch_request(request).await
}

/// A Core response value plus whether Core signalled that this
/// mutation's audit write failed (`header::AUDIT_WARNING`, FR-AUD-11).
/// Only the write paths this concerns construct one -- every read-only
/// `call_json` caller is unaffected and unaware this type exists.
pub struct AuditChecked<T> {
    pub value: T,
    pub audit_warning: bool,
}

async fn call_json_checked<T: for<'de> serde::Deserialize<'de>>(
    env: &Env,
    method: Method,
    path: &str,
    caller: Option<&Caller>,
    body: Option<&serde_json::Value>,
) -> Result<AuditChecked<T>> {
    let mut response = call(env, method, path, caller, body).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        let msg = response.text().await.unwrap_or_default();
        return Err(Error::RustError(format!(
            "Core returned {}: {}",
            response.status_code(),
            msg
        )));
    }
    let audit_warning = response.headers().get(header::AUDIT_WARNING)?.is_some();
    let text = response.text().await?;
    let value = serde_json::from_str(&text).map_err(|e| {
        Error::RustError(format!("Core response parse error: {} body: {}", e, text))
    })?;
    Ok(AuditChecked {
        value,
        audit_warning,
    })
}

async fn call_json<T: for<'de> serde::Deserialize<'de>>(
    env: &Env,
    method: Method,
    path: &str,
    caller: Option<&Caller>,
    body: Option<&serde_json::Value>,
) -> Result<T> {
    call_json_checked(env, method, path, caller, body)
        .await
        .map(|checked| checked.value)
}

// ─────────────────────────────────────────────
//  User lookup (used during authentication / no Caller required)
// ─────────────────────────────────────────────

pub async fn lookup_user(env: &Env, email: &str) -> Result<Option<User>> {
    let path = format!("/users/lookup/{}", urlencoding::encode(email));
    let result: LookupUserResult = call_json(env, Method::Get, &path, None, None).await?;
    Ok(result.user)
}

// ─────────────────────────────────────────────
//  Targets
// ─────────────────────────────────────────────

pub async fn list_targets(env: &Env, caller: &Caller) -> Result<Vec<Target>> {
    call_json(env, Method::Get, "/targets", Some(caller), None).await
}

pub async fn get_target(env: &Env, caller: &Caller, id: &str) -> Result<Target> {
    let path = format!("/targets/{}", id);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

pub async fn create_target(
    env: &Env,
    caller: &Caller,
    input: &CreateTargetInput,
) -> Result<AuditChecked<Target>> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(env, Method::Post, "/targets", Some(caller), Some(&body)).await
}

pub async fn update_target(
    env: &Env,
    caller: &Caller,
    id: &str,
    input: &UpdateTargetInput,
) -> Result<AuditChecked<Target>> {
    let path = format!("/targets/{}", id);
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(env, Method::Put, &path, Some(caller), Some(&body)).await
}

/// Returns whether Core signalled the delete's audit write failed
/// (`header::AUDIT_WARNING`) -- there is no response body to carry a
/// value in, so unlike the create/update siblings this isn't wrapped
/// in `AuditChecked`.
pub async fn delete_target(env: &Env, caller: &Caller, id: &str) -> Result<bool> {
    let path = format!("/targets/{}", id);
    let mut response = call(env, Method::Delete, &path, Some(caller), None).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core delete failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response.headers().get(header::AUDIT_WARNING)?.is_some())
}

pub async fn status_summary(env: &Env, caller: &Caller) -> Result<StatusSummary> {
    call_json(env, Method::Get, "/targets/summary", Some(caller), None).await
}

pub async fn list_states(env: &Env, caller: &Caller) -> Result<Vec<TargetState>> {
    call_json(env, Method::Get, "/targets/states", Some(caller), None).await
}

pub async fn get_state(env: &Env, caller: &Caller, id: &str) -> Result<TargetState> {
    let path = format!("/targets/{}/state", id);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

pub async fn list_results(
    env: &Env,
    caller: &Caller,
    id: &str,
    limit: i64,
) -> Result<Vec<CheckResult>> {
    let path = format!("/targets/{}/results?limit={}", id, limit);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

// ─────────────────────────────────────────────
//  Incidents
// ─────────────────────────────────────────────

pub async fn list_incidents(env: &Env, caller: &Caller, limit: i64) -> Result<Vec<Incident>> {
    let path = format!("/incidents?limit={}", limit);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

/// Returns whether Core signalled the resolve's audit write failed.
pub async fn resolve_incident(
    env: &Env,
    caller: &Caller,
    id: &str,
    input: &ResolveIncidentInput,
) -> Result<bool> {
    let path = format!("/incidents/{}/resolve", id);
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    let mut response = call(env, Method::Post, &path, Some(caller), Some(&body)).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core resolve failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response.headers().get(header::AUDIT_WARNING)?.is_some())
}

// ─────────────────────────────────────────────
//  Maintenance
// ─────────────────────────────────────────────

pub async fn list_maintenance(env: &Env, caller: &Caller) -> Result<Vec<MaintenanceWindow>> {
    call_json(env, Method::Get, "/maintenance", Some(caller), None).await
}

pub async fn create_maintenance(
    env: &Env,
    caller: &Caller,
    input: &CreateMaintenanceInput,
) -> Result<AuditChecked<MaintenanceWindow>> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(env, Method::Post, "/maintenance", Some(caller), Some(&body)).await
}

// ─────────────────────────────────────────────
//  Audit log
// ─────────────────────────────────────────────

pub async fn list_audit(env: &Env, caller: &Caller, limit: i64) -> Result<Vec<AuditEntry>> {
    let path = format!("/audit?limit={}", limit);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

/// Proxy to Core's `/audit/verify` endpoint. The result is returned as raw
/// `serde_json::Value` so the gateway can pass it through to the operator
/// without depending on Core's `ChainVerification` shape.
pub async fn verify_audit_chain(env: &Env, caller: &Caller) -> Result<serde_json::Value> {
    call_json(env, Method::Get, "/audit/verify", Some(caller), None).await
}

/// Fetch the calling user's own login history. Limit defaults to 20 on the
/// Core side; we don't bother round-tripping different values for now.
pub async fn login_history(env: &Env, caller: &Caller, limit: i64) -> Result<Vec<AuditEntry>> {
    let path = format!("/audit/login-history?limit={}", limit);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

/// Record a login event. Called by the OIDC callback right after a fresh
/// session is created. The just-logged-in user is identified by the body
/// because no caller header could exist yet (the session that would back
/// it has not been read by the user's browser).
pub async fn record_login(
    env: &Env,
    user_id: &str,
    user_email: &str,
    ip_address: Option<&str>,
) -> Result<()> {
    let body = serde_json::json!({
        "user_id": user_id,
        "user_email": user_email,
        "ip_address": ip_address,
    });
    let mut response = call(env, Method::Post, "/audit/login", None, Some(&body)).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        let body = response.text().await.unwrap_or_default();
        // Login-history is a nice-to-have; surface the error in console
        // but don't fail the login. The session is already valid.
        console_log!(
            "[record_login] core returned {}: {}",
            response.status_code(),
            body
        );
    }
    Ok(())
}

// ─────────────────────────────────────────────
//  User management
// ─────────────────────────────────────────────

pub async fn list_users(env: &Env, caller: &Caller) -> Result<Vec<User>> {
    call_json(env, Method::Get, "/users", Some(caller), None).await
}

pub async fn upsert_user(
    env: &Env,
    caller: &Caller,
    input: &ManageUserInput,
) -> Result<AuditChecked<User>> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(env, Method::Post, "/users", Some(caller), Some(&body)).await
}

// ─────────────────────────────────────────────
//  Notification channels
// ─────────────────────────────────────────────

pub async fn list_channels(env: &Env, caller: &Caller) -> Result<Vec<NotificationChannel>> {
    call_json(env, Method::Get, "/channels", Some(caller), None).await
}

pub async fn get_channel(env: &Env, caller: &Caller, id: &str) -> Result<NotificationChannel> {
    let path = format!("/channels/{}", id);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

pub async fn create_channel(
    env: &Env,
    caller: &Caller,
    input: &CreateNotificationChannelInput,
) -> Result<AuditChecked<NotificationChannel>> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(env, Method::Post, "/channels", Some(caller), Some(&body)).await
}

pub async fn update_channel(
    env: &Env,
    caller: &Caller,
    id: &str,
    input: &UpdateNotificationChannelInput,
) -> Result<AuditChecked<NotificationChannel>> {
    let path = format!("/channels/{}", id);
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(env, Method::Put, &path, Some(caller), Some(&body)).await
}

pub async fn delete_channel(env: &Env, caller: &Caller, id: &str) -> Result<bool> {
    let path = format!("/channels/{}", id);
    let mut response = call(env, Method::Delete, &path, Some(caller), None).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core delete_channel failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response.headers().get(header::AUDIT_WARNING)?.is_some())
}

pub async fn list_channels_for_target(
    env: &Env,
    caller: &Caller,
    target_id: &str,
) -> Result<Vec<AttachedChannel>> {
    let path = format!("/targets/{}/channels", target_id);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

pub async fn attach_channel(
    env: &Env,
    caller: &Caller,
    target_id: &str,
    input: &AttachChannelInput,
) -> Result<bool> {
    let path = format!("/targets/{}/channels", target_id);
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    let mut response = call(env, Method::Post, &path, Some(caller), Some(&body)).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core attach_channel failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response.headers().get(header::AUDIT_WARNING)?.is_some())
}

pub async fn detach_channel(
    env: &Env,
    caller: &Caller,
    target_id: &str,
    channel_id: &str,
) -> Result<bool> {
    let path = format!("/targets/{}/channels/{}", target_id, channel_id);
    let mut response = call(env, Method::Delete, &path, Some(caller), None).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core detach_channel failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response.headers().get(header::AUDIT_WARNING)?.is_some())
}

pub async fn test_channel(env: &Env, caller: &Caller, id: &str) -> Result<bool> {
    let path = format!("/channels/{}/test", id);
    let mut response = call(env, Method::Post, &path, Some(caller), None).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core test_channel failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(response.headers().get(header::AUDIT_WARNING)?.is_some())
}

pub async fn list_targets_for_channel(
    env: &Env,
    caller: &Caller,
    channel_id: &str,
) -> Result<Vec<AttachedTarget>> {
    let path = format!("/channels/{}/targets", channel_id);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

// ─────────────────────────────────────────────
//  SLA / availability reports
// ─────────────────────────────────────────────

pub async fn get_target_sla(
    env: &Env,
    caller: &Caller,
    target_id: &str,
    window: &str,
) -> Result<SlaReport> {
    let path = format!(
        "/targets/{}/sla?window={}",
        target_id,
        urlencoding::encode(window)
    );
    call_json(env, Method::Get, &path, Some(caller), None).await
}

/// Multi-window report — fetches 24h, 7d, and 30d in one round-trip.
pub async fn get_target_sla_multi(
    env: &Env,
    caller: &Caller,
    target_id: &str,
) -> Result<SlaMultiReport> {
    let path = format!("/targets/{}/sla/multi", target_id);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

pub async fn get_aggregate_sla(env: &Env, caller: &Caller, window: &str) -> Result<SlaSummary> {
    let path = format!("/stats/sla?window={}", urlencoding::encode(window));
    call_json(env, Method::Get, &path, Some(caller), None).await
}

/// Window-scoped incident list for one target. Mirrors the data the SLA
/// detail page needs to fill its "incidents in this window" section.
pub async fn list_target_incidents_in_window(
    env: &Env,
    caller: &Caller,
    target_id: &str,
    window: &str,
) -> Result<Vec<Incident>> {
    let path = format!(
        "/targets/{}/incidents?window={}",
        target_id,
        urlencoding::encode(window)
    );
    call_json(env, Method::Get, &path, Some(caller), None).await
}

// ─────────────────────────────────────────────
//  Configuration migration (export / import)
// ─────────────────────────────────────────────

pub async fn export_migration(
    env: &Env,
    caller: &Caller,
    include_users: bool,
) -> Result<AuditChecked<MigrationExport>> {
    let path = format!("/admin/migration/export?include_users={}", include_users);
    call_json_checked(env, Method::Get, &path, Some(caller), None).await
}

pub async fn import_migration(
    env: &Env,
    caller: &Caller,
    body: &ImportRequest,
) -> Result<AuditChecked<ImportResult>> {
    let json = serde_json::to_value(body)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json_checked(
        env,
        Method::Post,
        "/admin/migration/import",
        Some(caller),
        Some(&json),
    )
    .await
}
