//! Gateway と Core ワーカー間で共有される型定義。
//!
//! 両ワーカーは Service Binding 経由で HTTP (JSON ボディ) でやり取りするため、
//! シリアライズ形式が完全一致する必要がある。型を単一ソースに集約することで齟齬を防ぐ。

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
//  Caller (認証済みユーザー情報)
// ─────────────────────────────────────────────

/// ゲートウェイで OIDC 認証を通過した呼び出し元の情報。
///
/// Service Binding 経由で Core に伝搬する際は、HTTP ヘッダ (`X-Caller-*`) に
/// 分解してエンコードされる。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caller {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role: String, // "admin" | "member"
}

impl Caller {
    pub fn is_admin(&self) -> bool {
        self.role == "admin"
    }
}

// ─────────────────────────────────────────────
//  Target (監視対象)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub host: String,
    pub port: Option<i64>,
    pub path: Option<String>,
    pub expected_status: Option<i64>,
    pub body_contains: Option<String>,
    pub tls_threshold_days: Option<i64>,
    pub timeout_sec: i64,
    pub retry_count: i64,
    pub interval_minutes: i64,
    pub is_disabled: bool,
    pub owner_id: String,
    pub tags: Option<String>,
    pub next_check_at: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateTargetInput {
    pub name: String,
    #[serde(rename = "type")]
    pub target_type: String,
    pub host: String,
    pub port: Option<i64>,
    pub path: Option<String>,
    pub expected_status: Option<i64>,
    pub body_contains: Option<String>,
    pub tls_threshold_days: Option<i64>,
    pub timeout_sec: Option<i64>,
    pub retry_count: Option<i64>,
    pub interval_minutes: Option<i64>,
    pub tags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateTargetInput {
    pub name: Option<String>,
    pub host: Option<String>,
    pub port: Option<i64>,
    pub path: Option<String>,
    pub expected_status: Option<i64>,
    pub body_contains: Option<String>,
    pub tls_threshold_days: Option<i64>,
    pub timeout_sec: Option<i64>,
    pub retry_count: Option<i64>,
    pub interval_minutes: Option<i64>,
    pub is_disabled: Option<bool>,
    pub tags: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    pub total: i64,
    pub up: i64,
    pub down: i64,
    pub degraded: i64,
    pub maintenance: i64,
    pub unknown: i64,
    pub disabled: i64,
}

// ─────────────────────────────────────────────
//  TargetState (状態管理)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetState {
    pub target_id: String,
    pub current_status: String,
    pub consecutive_successes: i64,
    pub consecutive_failures: i64,
    pub success_threshold: i64,
    pub failure_threshold: i64,
    pub last_checked_at: Option<String>,
    pub last_status_change_at: Option<String>,
    pub last_notification_at: Option<String>,
}

// ─────────────────────────────────────────────
//  CheckResult (監視結果)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub id: String,
    pub target_id: String,
    pub checked_at: String,
    pub is_success: bool,
    pub status_code: Option<i64>,
    pub response_time_ms: Option<i64>,
    pub error_message: Option<String>,
    pub tls_expiry_date: Option<String>,
    pub tls_days_left: Option<i64>,
    pub details: Option<String>,
}

// ─────────────────────────────────────────────
//  Incident (障害イベント)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: String,
    pub target_id: String,
    pub status: String,
    pub opened_at: String,
    pub resolved_at: Option<String>,
    pub duration_sec: Option<i64>,
    pub cause: Option<String>,
    pub resolution_note: Option<String>,
    pub created_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveIncidentInput {
    pub note: Option<String>,
}

// ─────────────────────────────────────────────
//  MaintenanceWindow (メンテナンス期間)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaintenanceWindow {
    pub id: String,
    pub name: String,
    pub start_at: String,
    pub end_at: String,
    pub target_tag: Option<String>,
    pub target_id: Option<String>,
    pub suppress_notify: bool,
    pub is_active: bool,
    pub created_at: String,
    pub created_by: String,
    pub updated_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMaintenanceInput {
    pub name: String,
    pub start_at: String,
    pub end_at: String,
    pub target_tag: Option<String>,
    pub target_id: Option<String>,
    pub suppress_notify: Option<bool>,
}

// ─────────────────────────────────────────────
//  AuditEntry (監査ログ)
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub action_time: String,
    pub actor_id: String,
    pub actor_email: Option<String>,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub action_type: String,
    pub previous_value: Option<String>,
    pub new_value: Option<String>,
    pub result: String,
    pub ip_address: Option<String>,
}

// ─────────────────────────────────────────────
//  User
// ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManageUserInput {
    pub email: String,
    pub name: String,
    pub role: String,
    pub is_active: Option<bool>,
}

/// ユーザー lookup のレスポンス (Gateway が認証時に呼び出す)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupUserResult {
    pub user: Option<User>,
}

// ─────────────────────────────────────────────
//  HTTP Header キー (Gateway ↔ Core の規約)
// ─────────────────────────────────────────────

pub mod header {
    /// Gateway → Core 共有秘密 (二重防御)
    pub const GATEWAY_TOKEN: &str = "X-Gateway-Token";
    /// 呼び出し元ユーザー ID
    pub const CALLER_USER_ID: &str = "X-Caller-UserId";
    /// 呼び出し元 email
    pub const CALLER_EMAIL: &str = "X-Caller-Email";
    /// 呼び出し元ロール
    pub const CALLER_ROLE: &str = "X-Caller-Role";
    /// 呼び出し元表示名
    pub const CALLER_NAME: &str = "X-Caller-Name";
}
