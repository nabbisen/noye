//! Shared type definitions between the Gateway and Core workers.
//!
//! The two workers exchange HTTP (with JSON bodies) over a Service Binding, so
//! their serialization formats must match exactly. Centralizing the types here prevents drift.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
//  Caller (Authenticated user info)
// ─────────────────────────────────────────────

/// Information about a caller that has passed OIDC authentication at the Gateway.
///
/// When propagated to the Core through a Service Binding it is split into
/// the `X-Caller-*` HTTP headers.
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
//  Target (targets)
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
//  TargetState (State tracking)
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
//  CheckResult (Check results)
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
//  Incident (incidents)
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
//  MaintenanceWindow (maintenance windows)
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
//  AuditEntry (audit log)
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

/// Response shape for user lookup (called by the Gateway during authentication).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LookupUserResult {
    pub user: Option<User>,
}

// ─────────────────────────────────────────────
//  Notification channels
// ─────────────────────────────────────────────

/// A configured notification channel (webhook, email, or Slack incoming
/// webhook). Stored in the `notification_channels` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: String,
    pub name: String,
    pub channel_type: String, // "webhook" | "email" | "slack"
    pub endpoint: String,
    pub is_enabled: bool,
    pub owner_id: String,
    pub created_at: String,
}

/// Input to create a notification channel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateNotificationChannelInput {
    pub name: String,
    pub channel_type: String,
    pub endpoint: String,
}

/// Input to update an existing channel. All fields are optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateNotificationChannelInput {
    pub name: Option<String>,
    pub endpoint: Option<String>,
    pub is_enabled: Option<bool>,
}

/// Row from the `target_notifications` join table joined to channel metadata.
/// Returned by the "list channels attached to a given target" endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedChannel {
    pub channel_id: String,
    pub channel_name: String,
    pub channel_type: String,
    pub endpoint: String,
    pub is_enabled: bool,
    pub on_down: bool,
    pub on_up: bool,
}

/// Input to attach (or re-attach with updated triggers) a channel to a target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachChannelInput {
    pub channel_id: String,
    pub on_down: bool,
    pub on_up: bool,
}

/// Reverse lookup: a target that a given channel is attached to. Used by the
/// channel detail page to show "where will my changes have an effect?" before
/// the operator hits Save or Delete.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachedTarget {
    pub target_id: String,
    pub target_name: String,
    pub target_type: String,
    pub target_host: String,
    pub on_down: bool,
    pub on_up: bool,
}

// ─────────────────────────────────────────────
//  SLA / availability reporting
// ─────────────────────────────────────────────

/// A single target's availability over a window.
///
/// Two numbers are returned: `gross_uptime_ratio` includes downtime caused by
/// scheduled maintenance, while `sla_uptime_ratio` excludes it. Both are in
/// the range `0.0..=1.0`. Multiply by 100 for percentage display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaReport {
    pub target_id: String,
    pub target_name: String,
    pub window_start: String,
    pub window_end: String,
    pub window_seconds: i64,
    pub downtime_seconds: i64,
    pub maintenance_seconds: i64,
    pub gross_uptime_ratio: f64,
    pub sla_uptime_ratio: f64,
    pub incident_count: i64,
    pub mttr_seconds: Option<i64>, // Mean time to recovery (None when no resolved incidents)
}

/// Aggregate report covering every visible target.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaSummary {
    pub window_start: String,
    pub window_end: String,
    pub window_seconds: i64,
    pub per_target: Vec<SlaReport>,
    pub overall_gross_uptime_ratio: f64,
    pub overall_sla_uptime_ratio: f64,
}

/// SLA reports for a single target across multiple windows simultaneously.
/// Used by the per-target detail page so the operator can see at a glance
/// whether short-term and long-term reliability tell the same story.
///
/// The wire format is a list of `(window_label, report)` pairs rather than a
/// fixed-shape struct so that future windows (e.g. "1y") can be added without
/// breaking deserialization on older Gateway builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaMultiReport {
    pub target_id: String,
    pub target_name: String,
    pub windows: Vec<SlaWindowEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaWindowEntry {
    /// Human-readable window label (e.g. "24h", "7d", "30d"). Mirrors the
    /// `?window=` query-string accepted by `parse_window`.
    pub label: String,
    pub report: SlaReport,
}

// ─────────────────────────────────────────────
//  Migration: configuration export / import
// ─────────────────────────────────────────────

/// Top-level wire format for `GET /api/admin/export` and the symmetric input
/// to `POST /api/admin/import`. The `schema_version` is bumped whenever a
/// breaking change to this shape is introduced; importers reject payloads
/// whose version they don't understand rather than silently dropping fields.
///
/// Bulk monitoring data (`check_results`, `incidents`, `audit_logs`, R2
/// archive snapshots) is intentionally NOT included — those are time-series
/// volumes that are better moved with `wrangler d1 export` than over a
/// Workers HTTP request. See `docs/src/migration.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationExport {
    pub schema_version: u32,
    pub exported_at: String,
    pub source_deployment: Option<String>, // Optional human-readable label
    pub data: MigrationData,
}

/// Per-table data carried by an export. Vectors of complete records ready
/// to be re-inserted on the destination. `users` is `None` when the operator
/// opted out at export time, so an importer can distinguish "no users in
/// source" from "users were not exported."
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationData {
    pub targets: Vec<Target>,
    pub channels: Vec<NotificationChannel>,
    pub target_notifications: Vec<TargetNotificationLink>,
    pub maintenance_windows: Vec<MaintenanceWindow>,
    pub users: Option<Vec<User>>,
}

/// A target ↔ channel attachment, in a form suitable for export. Mirrors
/// what's in the `target_notifications` join table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TargetNotificationLink {
    pub target_id: String,
    pub channel_id: String,
    pub on_down: bool,
    pub on_up: bool,
}

/// Behavior on ID conflict during import. The default for an empty
/// destination is `Skip`; for a reset/restore use case the operator opts
/// into `Replace`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ImportConflictPolicy {
    /// Keep existing rows and ignore incoming rows with the same primary key.
    #[default]
    Skip,
    /// Overwrite existing rows with incoming data on PK collision.
    Replace,
    /// Stop and roll back the entire import on the first conflict.
    Fail,
}

/// Input to `POST /api/admin/import`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportRequest {
    pub payload: MigrationExport,
    #[serde(default)]
    pub on_conflict: ImportConflictPolicy,
    /// Set to `true` to actually write to D1. When `false` (the default), the
    /// server validates the payload and returns the row counts that *would*
    /// be written without making any changes. Treats the import like a
    /// `--dry-run` flag in CLI tools.
    #[serde(default)]
    pub apply: bool,
}

/// Response shape for both dry-run and real imports. Counts are per-table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportResult {
    pub applied: bool,
    pub schema_version: u32,
    pub conflict_policy: ImportConflictPolicy,
    pub rows: ImportRowCounts,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ImportRowCounts {
    pub targets: i64,
    pub channels: i64,
    pub target_notifications: i64,
    pub maintenance_windows: i64,
    pub users: i64,
    pub skipped: i64,
    pub replaced: i64,
}

/// Schema version for the migration payload. Bump on any breaking change to
/// the on-the-wire shape (renaming fields, removing fields, changing types).
pub const MIGRATION_SCHEMA_VERSION: u32 = 1;

// ─────────────────────────────────────────────
//  HTTP header names (Gateway ↔ Core protocol)
// ─────────────────────────────────────────────

pub mod header {
    /// Shared secret from Gateway to Core (defense in depth)
    pub const GATEWAY_TOKEN: &str = "X-Gateway-Token";
    /// Caller user ID
    pub const CALLER_USER_ID: &str = "X-Caller-UserId";
    /// Caller email
    pub const CALLER_EMAIL: &str = "X-Caller-Email";
    /// Caller role
    pub const CALLER_ROLE: &str = "X-Caller-Role";
    /// Caller display name
    pub const CALLER_NAME: &str = "X-Caller-Name";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caller_is_admin_only_for_admin_role() {
        let admin = Caller {
            user_id: "u".to_string(),
            email: "a@b".to_string(),
            name: "A".to_string(),
            role: "admin".to_string(),
        };
        assert!(admin.is_admin());

        let member = Caller {
            role: "member".to_string(),
            ..admin.clone()
        };
        assert!(!member.is_admin());

        let guest = Caller {
            role: "guest".to_string(),
            ..admin.clone()
        };
        assert!(!guest.is_admin());

        let empty = Caller {
            role: String::new(),
            ..admin
        };
        assert!(!empty.is_admin());
    }

    #[test]
    fn header_constants_match_protocol_spec() {
        // These header names are part of the Gateway-Core wire contract.
        // Both workers reference the same constants, but pin the values here
        // to catch accidental renames.
        assert_eq!(header::GATEWAY_TOKEN, "X-Gateway-Token");
        assert_eq!(header::CALLER_USER_ID, "X-Caller-UserId");
        assert_eq!(header::CALLER_EMAIL, "X-Caller-Email");
        assert_eq!(header::CALLER_ROLE, "X-Caller-Role");
        assert_eq!(header::CALLER_NAME, "X-Caller-Name");
    }

    #[test]
    fn caller_round_trips_through_json() {
        let caller = Caller {
            user_id: "u-1".to_string(),
            email: "u@example.com".to_string(),
            name: "Test".to_string(),
            role: "admin".to_string(),
        };
        let json = serde_json::to_string(&caller).unwrap();
        let back: Caller = serde_json::from_str(&json).unwrap();
        assert_eq!(back.user_id, caller.user_id);
        assert_eq!(back.email, caller.email);
        assert_eq!(back.name, caller.name);
        assert_eq!(back.role, caller.role);
    }
}
