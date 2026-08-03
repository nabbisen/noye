//! Shared type definitions between the Gateway and Core workers.
//!
//! The two workers exchange HTTP (with JSON bodies) over a Service Binding, so
//! their serialization formats must match exactly. Centralizing the types here prevents drift.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────
//  D1 boolean deserialization (subject 07b, G-36)
// ─────────────────────────────────────────────

/// Deserialize a `bool` from whatever shape it actually arrives in.
///
/// SQLite has no boolean type — every `bool`-typed column here is
/// stored as `INTEGER`, and D1 surfaces it to the Worker as a JS
/// `Number` (specifically `f64`; JS has exactly one number type), which
/// `serde`'s default `bool` deserialization does not accept. Apply as
/// `#[serde(deserialize_with = "bool_from_d1")]` on every field backed
/// by such a column.
///
/// **Not an untagged `enum { Bool(bool), Number(i64) }`.** That was
/// this subject's first draft, and it does not work: an untagged enum
/// with an `i64` arm rejects a float outright ("data did not match any
/// variant"), and JS numbers are always `f64` — never `i64` — so the
/// numeric arm never matches and the mismatch still panics one level
/// up in `worker`'s own `D1Result::results`/`first` (which call
/// `.unwrap()` on the deserialize result). A `Visitor` accepts
/// whichever numeric type the deserializer actually presents rather
/// than requiring the caller to predict it correctly, and it also
/// drops the untagged enum's buffering layer (serde's `Content`),
/// which is one more thing that would otherwise need to behave
/// identically between `serde_json` and `serde_wasm_bindgen` for this
/// to work — an assumption this project no longer takes on faith.
///
/// Truthiness is `!= 0`, not `== 1`: SQLite (and D1) truthiness is
/// non-zero, and `== 1` would silently read a stray `2` as `false`.
/// `NaN` is rejected as an error rather than read as `true` —
/// `NaN != 0.0` is `true` in IEEE 754, which is exactly the silent
/// inversion this function exists to prevent, arriving through a case
/// nothing currently writes but nothing should read past either.
pub fn bool_from_d1<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolFromD1;

    impl serde::de::Visitor<'_> for BoolFromD1 {
        type Value = bool;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a boolean, or the integer/float SQLite stores one as (non-zero is true)")
        }

        fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }

        fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<bool, E> {
            if v.is_nan() {
                return Err(serde::de::Error::custom(
                    "cannot interpret NaN as a boolean",
                ));
            }
            Ok(v != 0.0)
        }
    }

    deserializer.deserialize_any(BoolFromD1)
}

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
    #[serde(deserialize_with = "bool_from_d1")]
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
    #[serde(deserialize_with = "bool_from_d1")]
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
    #[serde(deserialize_with = "bool_from_d1")]
    pub suppress_notify: bool,
    #[serde(deserialize_with = "bool_from_d1")]
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
    #[serde(deserialize_with = "bool_from_d1")]
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
    #[serde(deserialize_with = "bool_from_d1")]
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
    /// Set on a mutation's response when the business change succeeded but
    /// its audit record failed to write (FR-AUD-08, FR-AUD-11, DEC-011,
    /// G-26). Presence is the signal, not the value -- set to "1" wherever
    /// it appears. Core sets it on its response to the Gateway; the
    /// Gateway relays it, unchanged, on its own response to the browser.
    pub const AUDIT_WARNING: &str = "X-Audit-Warning";
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
        assert_eq!(header::AUDIT_WARNING, "X-Audit-Warning");
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

/// D1 deserialization tests (subject 07b, G-36).
///
/// `worker`'s `D1Result::results`/`first` (Core-side) deserialize a JS
/// value into a Rust struct via `serde_wasm_bindgen`. SQLite has no
/// boolean type -- every `bool`-typed column here is stored as
/// `INTEGER`, and D1 surfaces it as a JS `Number`, which
/// `serde_wasm_bindgen` does not coerce into a Rust `bool`. These
/// tests reproduce that directly, one per affected field, by
/// constructing the exact JS shape D1 hands back (via `JSON.parse`,
/// the same route worker uses internally) and running the identical
/// deserializer.
///
/// This lives in `noye-shared`, not `noye-core` where these structs
/// are actually read from D1, because `noye-core` links
/// `wasm-smtp-cloudflare` (for SMTP delivery), which references a
/// `cloudflare:`-scheme JS import that Node's ESM loader rejects
/// outright at module-load time -- confirmed directly: an identical
/// test placed in `noye-core` failed with `ERR_UNSUPPORTED_ESM_URL_SCHEME`
/// regardless of which test name was requested, before any test body
/// even ran. `noye-shared` has no such dependency, so its wasm test
/// binary loads cleanly. `RetentionPolicy` (private to
/// `noye-core::db::retention`, so it can't move here) is confirmed by
/// the live reproduction already captured in
/// `.git-exclude/evidence/subject-07a-run-cleanup-panic-finding.log`
/// instead -- arguably stronger evidence, since it is the actual
/// crash, not a simulation of it.
///
/// Run with: `cargo test -p noye-shared --target wasm32-unknown-unknown`
/// -- plain host `cargo test` cannot even construct a `JsValue`
/// (confirmed: a throwaway probe aborted the process with a
/// non-unwinding panic outside a real JS engine), so this needs the
/// wasm32 target run through Node, not the host `cargo test` the
/// handoff's own wording suggests. Calling `serde_wasm_bindgen::
/// from_value` directly, as these tests do, returns a clean `Result` --
/// no crash -- because the crash in the real request path comes from
/// `worker`'s own internal `.unwrap()` on that `Result`
/// (`worker-0.8.5/src/d1/mod.rs:491`), not from the deserializer
/// itself.
#[cfg(test)]
mod d1_bool_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    /// Parse a JSON literal into a `JsValue` via the JS engine's own
    /// `JSON.parse` -- the same route through which a JS number
    /// (`1`/`0`) for a SQLite `INTEGER` column arrives as a genuine JS
    /// `Number`, not a Rust-side fabrication of one.
    fn parse_js(json: &str) -> wasm_bindgen::JsValue {
        js_sys::JSON::parse(json).expect("fixture JSON must itself be valid")
    }

    // ── T-189 — each field deserializes correctly from a JS number ──
    // (must fail first: today, every one of these is `Err`)

    #[wasm_bindgen_test]
    fn t189_user_is_active_from_a_json_number() {
        let value = parse_js(
            r#"{"id":"u-1","email":"a@b.c","name":"A","role":"admin","is_active":1,
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        );
        let user: Result<User, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            user.is_ok(),
            "User.is_active must deserialize from a JS number (D1's real shape): {:?}",
            user.err()
        );
    }

    #[wasm_bindgen_test]
    fn t189_target_is_disabled_from_a_json_number() {
        let value = parse_js(
            r#"{"id":"t-1","name":"T","type":"https","host":"example.com","port":null,
                "path":null,"expected_status":null,"body_contains":null,
                "tls_threshold_days":null,"timeout_sec":10,"retry_count":3,
                "interval_minutes":5,"is_disabled":0,"owner_id":"u-1","tags":null,
                "next_check_at":"2026-01-01T00:00:00Z","created_at":"2026-01-01T00:00:00Z",
                "updated_at":"2026-01-01T00:00:00Z","created_by":"u-1","updated_by":"u-1"}"#,
        );
        let target: Result<Target, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            target.is_ok(),
            "Target.is_disabled must deserialize from a JS number (D1's real shape): {:?}",
            target.err()
        );
    }

    #[wasm_bindgen_test]
    fn t189_check_result_is_success_from_a_json_number() {
        let value = parse_js(
            r#"{"id":"cr-1","target_id":"t-1","checked_at":"2026-01-01T00:00:00Z",
                "is_success":1,"status_code":200,"response_time_ms":50,
                "error_message":null,"tls_expiry_date":null,"tls_days_left":null,
                "details":null}"#,
        );
        let result: Result<CheckResult, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            result.is_ok(),
            "CheckResult.is_success must deserialize from a JS number (D1's real shape): {:?}",
            result.err()
        );
    }

    #[wasm_bindgen_test]
    fn t189_maintenance_window_is_active_from_a_json_number() {
        let value = parse_js(
            r#"{"id":"mw-1","name":"M","start_at":"2026-01-01T00:00:00Z",
                "end_at":"2026-01-02T00:00:00Z","target_tag":null,"target_id":null,
                "suppress_notify":1,"is_active":1,"created_at":"2026-01-01T00:00:00Z",
                "created_by":"u-1","updated_by":"u-1"}"#,
        );
        let window: Result<MaintenanceWindow, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            window.is_ok(),
            "MaintenanceWindow.is_active must deserialize from a JS number (D1's real shape): {:?}",
            window.err()
        );
    }

    #[wasm_bindgen_test]
    fn t189_maintenance_window_suppress_notify_from_a_json_number() {
        // Same struct as above, isolated as its own named assertion per
        // the handoff's "one assertion per field" -- a fix that happens
        // to work for is_active but not suppress_notify (or vice versa)
        // must be caught by its own failure, not inferred from a
        // sibling field.
        let value = parse_js(
            r#"{"id":"mw-1","name":"M","start_at":"2026-01-01T00:00:00Z",
                "end_at":"2026-01-02T00:00:00Z","target_tag":null,"target_id":null,
                "suppress_notify":0,"is_active":1,"created_at":"2026-01-01T00:00:00Z",
                "created_by":"u-1","updated_by":"u-1"}"#,
        );
        let window: Result<MaintenanceWindow, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            window.is_ok(),
            "MaintenanceWindow.suppress_notify must deserialize from a JS number (D1's real shape): {:?}",
            window.err()
        );
    }

    #[wasm_bindgen_test]
    fn t189_notification_channel_is_enabled_from_a_json_number() {
        let value = parse_js(
            r#"{"id":"ch-1","name":"C","channel_type":"webhook",
                "endpoint":"https://example.com/hook","is_enabled":1,
                "owner_id":"u-1","created_at":"2026-01-01T00:00:00Z"}"#,
        );
        let channel: Result<NotificationChannel, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            channel.is_ok(),
            "NotificationChannel.is_enabled must deserialize from a JS number (D1's real shape): {:?}",
            channel.err()
        );
    }

    // ── T-190 — the same fields also accept a genuine JS boolean, so
    //    the eventual fix is not one-directional ──

    #[wasm_bindgen_test]
    fn t190_user_is_active_from_a_genuine_json_boolean() {
        let value = parse_js(
            r#"{"id":"u-1","email":"a@b.c","name":"A","role":"admin","is_active":true,
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        );
        let user: Result<User, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            user.is_ok(),
            "User.is_active must also accept a genuine JS boolean: {:?}",
            user.err()
        );
    }

    // ── T-191 — 0 -> false, 1 -> true, for every field. A helper that
    //    returns `true` unconditionally would still pass T-189. ──

    #[wasm_bindgen_test]
    fn t191_user_is_active_maps_0_and_1_correctly() {
        let zero = parse_js(
            r#"{"id":"u-1","email":"a@b.c","name":"A","role":"admin","is_active":0,
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        );
        let one = parse_js(
            r#"{"id":"u-1","email":"a@b.c","name":"A","role":"admin","is_active":1,
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        );
        let zero: User = serde_wasm_bindgen::from_value(zero).expect("0 must deserialize");
        let one: User = serde_wasm_bindgen::from_value(one).expect("1 must deserialize");
        assert!(!zero.is_active, "0 must map to false, not true");
        assert!(one.is_active, "1 must map to true, not false");
    }

    // ── T-191 (extended per ruling 038 §4) — a stray non-0/non-1
    //    integer must still read as true (n != 0, not n == 1), and NaN
    //    must be rejected rather than silently read as true. ──

    #[wasm_bindgen_test]
    fn t191_user_is_active_treats_a_stray_integer_as_true() {
        let value = parse_js(
            r#"{"id":"u-1","email":"a@b.c","name":"A","role":"admin","is_active":2,
                "created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}"#,
        );
        let user: User = serde_wasm_bindgen::from_value(value).expect("2 must deserialize");
        assert!(
            user.is_active,
            "a stray non-0/non-1 integer (2) must map to true, not silently to false (n != 0, not n == 1)"
        );
    }

    #[wasm_bindgen_test]
    fn t191_user_is_active_rejects_nan_rather_than_reading_it_as_true() {
        // JSON has no NaN literal (JSON.parse itself would reject a bare
        // `NaN` token), so the fixture is built field-by-field via
        // `Reflect::set` instead, to get a genuine `f64::NAN` into the
        // JsValue the way D1's underlying JS runtime could (e.g. from a
        // stray floating-point computation) -- the deserializer must not
        // treat it as truthy.
        let value: wasm_bindgen::JsValue = js_sys::Object::new().into();
        js_sys::Reflect::set(&value, &"id".into(), &"u-1".into()).unwrap();
        js_sys::Reflect::set(&value, &"email".into(), &"a@b.c".into()).unwrap();
        js_sys::Reflect::set(&value, &"name".into(), &"A".into()).unwrap();
        js_sys::Reflect::set(&value, &"role".into(), &"admin".into()).unwrap();
        js_sys::Reflect::set(&value, &"is_active".into(), &f64::NAN.into()).unwrap();
        js_sys::Reflect::set(&value, &"created_at".into(), &"2026-01-01T00:00:00Z".into()).unwrap();
        js_sys::Reflect::set(&value, &"updated_at".into(), &"2026-01-01T00:00:00Z".into()).unwrap();
        let user: Result<User, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            user.is_err(),
            "NaN must be rejected as an error, not silently read as true (NaN != 0.0 is true in IEEE 754)"
        );
    }
}
