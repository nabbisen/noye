//! Core ワーカーへの Service Binding クライアント。
//!
//! Gateway は D1 に直接触れず、全てのデータ操作を Core 経由で行う。
//! このモジュールは Core の内部 REST API を型安全にラップする。
//!
//! 各リクエストには以下のヘッダが自動付与される:
//! - `X-Gateway-Token`: 共有秘密 (Core 側で検証)
//! - `X-Caller-*`: 認証済みユーザー情報 (Caller 引数が渡された場合)

use noye_shared::{
    header, AuditEntry, Caller, CheckResult, CreateMaintenanceInput, CreateTargetInput, Incident,
    LookupUserResult, MaintenanceWindow, ManageUserInput, ResolveIncidentInput, StatusSummary,
    Target, TargetState, UpdateTargetInput, User,
};
use worker::*;

const CORE_BINDING: &str = "CORE";

/// Core への内部 HTTP 呼び出しの基底 URL。
/// Service Binding 経由では URL の host 部分は無視されるが、worker-rs は
/// 何らかの有効な URL を要求するため `https://core.internal` を使用する。
const CORE_BASE_URL: &str = "https://core.internal";

/// Core への HTTP 呼び出しを実行する。
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

    let mut headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    headers.set("Accept", "application/json")?;

    // Gateway Token の伝搬
    if let Ok(secret) = env.secret("GATEWAY_SHARED_TOKEN") {
        headers.set(header::GATEWAY_TOKEN, &secret.to_string())?;
    } else if let Ok(token) = env.var("GATEWAY_SHARED_TOKEN") {
        headers.set(header::GATEWAY_TOKEN, &token.to_string())?;
    }

    // Caller 情報の伝搬
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

async fn call_json<T: for<'de> serde::Deserialize<'de>>(
    env: &Env,
    method: Method,
    path: &str,
    caller: Option<&Caller>,
    body: Option<&serde_json::Value>,
) -> Result<T> {
    let mut response = call(env, method, path, caller, body).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        let msg = response.text().await.unwrap_or_default();
        return Err(Error::RustError(format!(
            "Core returned {}: {}",
            response.status_code(),
            msg
        )));
    }
    let text = response.text().await?;
    serde_json::from_str(&text)
        .map_err(|e| Error::RustError(format!("Core response parse error: {} body: {}", e, text)))
}

// ─────────────────────────────────────────────
//  ユーザー照会 (認証時に使用 / Caller 不要)
// ─────────────────────────────────────────────

pub async fn lookup_user(env: &Env, email: &str) -> Result<Option<User>> {
    let path = format!("/users/lookup/{}", urlencoding::encode(email));
    let result: LookupUserResult = call_json(env, Method::Get, &path, None, None).await?;
    Ok(result.user)
}

// ─────────────────────────────────────────────
//  監視対象
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
) -> Result<Target> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json(env, Method::Post, "/targets", Some(caller), Some(&body)).await
}

pub async fn update_target(
    env: &Env,
    caller: &Caller,
    id: &str,
    input: &UpdateTargetInput,
) -> Result<Target> {
    let path = format!("/targets/{}", id);
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json(env, Method::Put, &path, Some(caller), Some(&body)).await
}

pub async fn delete_target(env: &Env, caller: &Caller, id: &str) -> Result<()> {
    let path = format!("/targets/{}", id);
    let mut response = call(env, Method::Delete, &path, Some(caller), None).await?;
    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Core delete failed: {} {}",
            response.status_code(),
            response.text().await.unwrap_or_default()
        )));
    }
    Ok(())
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
//  インシデント
// ─────────────────────────────────────────────

pub async fn list_incidents(env: &Env, caller: &Caller, limit: i64) -> Result<Vec<Incident>> {
    let path = format!("/incidents?limit={}", limit);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

pub async fn resolve_incident(
    env: &Env,
    caller: &Caller,
    id: &str,
    input: &ResolveIncidentInput,
) -> Result<()> {
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
    Ok(())
}

// ─────────────────────────────────────────────
//  メンテナンス
// ─────────────────────────────────────────────

pub async fn list_maintenance(env: &Env, caller: &Caller) -> Result<Vec<MaintenanceWindow>> {
    call_json(env, Method::Get, "/maintenance", Some(caller), None).await
}

pub async fn create_maintenance(
    env: &Env,
    caller: &Caller,
    input: &CreateMaintenanceInput,
) -> Result<MaintenanceWindow> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json(env, Method::Post, "/maintenance", Some(caller), Some(&body)).await
}

// ─────────────────────────────────────────────
//  監査ログ
// ─────────────────────────────────────────────

pub async fn list_audit(env: &Env, caller: &Caller, limit: i64) -> Result<Vec<AuditEntry>> {
    let path = format!("/audit?limit={}", limit);
    call_json(env, Method::Get, &path, Some(caller), None).await
}

// ─────────────────────────────────────────────
//  ユーザー管理
// ─────────────────────────────────────────────

pub async fn list_users(env: &Env, caller: &Caller) -> Result<Vec<User>> {
    call_json(env, Method::Get, "/users", Some(caller), None).await
}

pub async fn upsert_user(env: &Env, caller: &Caller, input: &ManageUserInput) -> Result<User> {
    let body = serde_json::to_value(input)
        .map_err(|e| Error::RustError(format!("input serialize error: {}", e)))?;
    call_json(env, Method::Post, "/users", Some(caller), Some(&body)).await
}
