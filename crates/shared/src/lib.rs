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
//  D1 i64 bind encoding (subject 07c, G-38)
// ─────────────────────────────────────────────

/// The largest (and, negated, the smallest) `i64` an `f64` represents
/// exactly. `2^53`: every integer magnitude up to and including this one
/// round-trips through a JS Number without loss; the next one up does not.
const D1_SAFE_INT_MAX: i64 = 1i64 << 53;

/// Convert an `i64` to the `JsValue` a D1 bind parameter will accept.
///
/// `wasm-bindgen` converts Rust integers to `JsValue` two different ways
/// (`wasm-bindgen-0.2.122/src/lib.rs`): `i8..u32` go through
/// `JsValue::from_f64` (a JS Number), and `i64`/`u64` go through a JS
/// `BigInt`. **D1's bind validation rejects a `BigInt` outright** —
/// `D1_TYPE_ERROR: Type 'bigint' not supported` — so `JsValue::from(<an
/// i64>)` fails at runtime for every value, not just large ones. This
/// function builds the JS Number directly instead, sidestepping the
/// `BigInt` path entirely.
///
/// **Rejects rather than truncates** anything outside `±2^53`. A `… as
/// i32` cast would avoid the `BigInt` path too, but would silently store
/// a different number than the caller passed for any value `i32` cannot
/// hold, and an `f64` cast alone would silently lose precision beyond
/// `2^53` while still "succeeding" — either is the wrong trade for a
/// monitoring system's numbers. Nothing bound through this project's
/// current call sites plausibly exceeds `2^53`; if a future one might,
/// that is a case for the caller to design for, not for this function to
/// paper over.
pub fn i64_to_d1(v: i64) -> Result<wasm_bindgen::JsValue, String> {
    if !(-D1_SAFE_INT_MAX..=D1_SAFE_INT_MAX).contains(&v) {
        return Err(format!(
            "{v} is outside ±2^53 ({D1_SAFE_INT_MAX}), the range an f64 \
             represents exactly -- refusing to silently store a different \
             number than was passed"
        ));
    }
    Ok(wasm_bindgen::JsValue::from_f64(v as f64))
}

/// [`i64_to_d1`] for an `Option<i64>` bind site: `Some` converts (and can
/// still be rejected for being out of range), `None` binds SQL `NULL`.
pub fn opt_i64_to_d1(v: Option<i64>) -> Result<wasm_bindgen::JsValue, String> {
    match v {
        Some(v) => i64_to_d1(v),
        None => Ok(wasm_bindgen::JsValue::NULL),
    }
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
    /// Subject 08 (G-05): who created/last updated this target. On the
    /// normal path this is always the acting caller; on import it is
    /// always the *importing* caller, never a value from the document
    /// (see `docs/src/external-design.md` §8.2).
    pub created_by: String,
    pub updated_by: String,
    /// Subject 10 (G-06, RFC 0008): decision configuration, not state --
    /// moved here from `target_states` so it round-trips through
    /// export/import like every other decision criterion.
    pub success_threshold: i64,
    pub failure_threshold: i64,
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
    pub success_threshold: Option<i64>,
    pub failure_threshold: Option<i64>,
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
    pub success_threshold: Option<i64>,
    pub failure_threshold: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusSummary {
    pub total: i64,
    pub up: i64,
    pub down: i64,
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
    /// Subject 16 (G-29): split from the single `created_by` column,
    /// which was overwritten at resolve and so meant "opener" for open
    /// rows and "resolver" for resolved ones. `open()` takes no caller
    /// and always writes the literal `"system"` -- no route opens an
    /// incident manually today -- so this is `Some("system")` for every
    /// row, but the point of the split is that resolving no longer
    /// clobbers it.
    pub opened_by: Option<String>,
    pub resolved_by: Option<String>,
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
    /// Subject 11 (G-07, DEC-013): independent of `suppress_notify` --
    /// whether this window's time is excluded from the SLA denominator.
    /// Defaults to 1 (excluded), matching the "Planned maintenance"
    /// situation and today's *intended* (if not actually implemented)
    /// behaviour.
    #[serde(deserialize_with = "bool_from_d1")]
    pub exclude_from_sla: bool,
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
    pub exclude_from_sla: Option<bool>,
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
    /// Subject 19 (G-16): the OIDC `sub` claim, once backfilled. `None`
    /// for a row no one has logged into since migration `0010` added
    /// this column -- the identity provider's subject identifier,
    /// deployment-specific, is never carried across a configuration
    /// export/import (`db/migration.rs::upsert_user` never binds it);
    /// a fresh login always re-resolves and backfills it locally.
    pub sub: Option<String>,
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

/// Request shape for identity resolution at login (subject 19, G-16).
/// `sub` is the OIDC subject claim from the just-verified ID token,
/// always present per the OIDC spec; `email` is the claim used only as
/// a one-time fallback to backfill a pre-existing, not-yet-claimed row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveIdentityInput {
    pub sub: String,
    pub email: String,
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
    /// Subject 13 (G-12, DEC-013): seconds excluded from the SLA
    /// denominator by an `exclude_from_sla` maintenance window. Renamed
    /// from `maintenance_seconds` -- this is no longer "time in any
    /// maintenance window", only the subset that excludes.
    pub excluded_seconds: i64,
    pub gross_uptime_ratio: f64,
    /// `None` when the entire window was excluded -- there is no
    /// measured availability to report, and reporting 100% would be a
    /// claim about a period nothing was measured over (FR-SLA-09).
    /// Same convention as `mttr_seconds`.
    pub sla_uptime_ratio: Option<f64>,
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
    /// `None` only when every target's window was fully excluded --
    /// see `SlaReport::sla_uptime_ratio`.
    pub overall_sla_uptime_ratio: Option<f64>,
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
                "updated_at":"2026-01-01T00:00:00Z","created_by":"u-1","updated_by":"u-1",
                "success_threshold":3,"failure_threshold":3}"#,
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
                "suppress_notify":1,"exclude_from_sla":1,"is_active":1,
                "created_at":"2026-01-01T00:00:00Z",
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
    fn t189a_maintenance_window_exclude_from_sla_from_a_json_number() {
        // Subject 11 (G-07) added `exclude_from_sla` as a third boolean on
        // this struct. Adding it without its own assertion is what broke
        // the two tests above on merge: they are fixture-based, so a new
        // required field makes them fail for a *missing field* reason
        // rather than the bool-conversion reason they exist to test --
        // which reads as "G-36 reopened" until you look. Every boolean
        // added to a struct D1 deserializes into needs its own named
        // assertion here, for the same reason the sibling comment gives.
        let value = parse_js(
            r#"{"id":"mw-1","name":"M","start_at":"2026-01-01T00:00:00Z",
                "end_at":"2026-01-02T00:00:00Z","target_tag":null,"target_id":null,
                "suppress_notify":1,"exclude_from_sla":0,"is_active":1,
                "created_at":"2026-01-01T00:00:00Z",
                "created_by":"u-1","updated_by":"u-1"}"#,
        );
        let window: Result<MaintenanceWindow, _> = serde_wasm_bindgen::from_value(value);
        assert!(
            window.is_ok(),
            "MaintenanceWindow.exclude_from_sla must deserialize from a JS number (D1's real shape): {:?}",
            window.err()
        );
        assert!(
            !window.unwrap().exclude_from_sla,
            "a JS 0 must read as false, not merely deserialize"
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
                "suppress_notify":0,"exclude_from_sla":1,"is_active":1,
                "created_at":"2026-01-01T00:00:00Z",
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

#[cfg(test)]
mod d1_i64_tests {
    use super::*;
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    // ── T-194 — the helper converts 0, 1, and a large in-range i64 to
    //    values D1 accepts, and rejects anything beyond 2^53 rather
    //    than truncating. The reject half matters more than the accept
    //    half: a helper that truncated would pass every other test in
    //    this subject while storing a different number than the
    //    operator entered. ──

    #[wasm_bindgen_test]
    fn t194_converts_zero() {
        let v = i64_to_d1(0).expect("0 is in range");
        assert_eq!(v.as_f64(), Some(0.0));
    }

    #[wasm_bindgen_test]
    fn t194_converts_one() {
        let v = i64_to_d1(1).expect("1 is in range");
        assert_eq!(v.as_f64(), Some(1.0));
    }

    #[wasm_bindgen_test]
    fn t194_converts_a_large_in_range_value() {
        // Comfortably larger than anything this project's D1 columns
        // hold today (timeouts, counts, status codes), still well
        // inside +-2^53.
        let large = 1_000_000_000_000_i64;
        let v = i64_to_d1(large).expect("1e12 is well within +-2^53");
        assert_eq!(v.as_f64(), Some(large as f64));
    }

    #[wasm_bindgen_test]
    fn t194_accepts_the_exact_boundary_2_pow_53() {
        let boundary = 1i64 << 53;
        let v = i64_to_d1(boundary).expect("2^53 itself is exactly representable");
        assert_eq!(v.as_f64(), Some(boundary as f64));
    }

    #[wasm_bindgen_test]
    fn t194_rejects_one_past_the_boundary_rather_than_truncating() {
        let just_over = (1i64 << 53) + 1;
        let err =
            i64_to_d1(just_over).expect_err("2^53 + 1 must be rejected, not silently rounded");
        assert!(err.contains("2^53"), "error should explain why: {err}");
    }

    #[wasm_bindgen_test]
    fn t194_rejects_the_negative_boundary_symmetrically() {
        let just_under = -(1i64 << 53) - 1;
        assert!(
            i64_to_d1(just_under).is_err(),
            "-2^53 - 1 must be rejected too"
        );
    }

    #[wasm_bindgen_test]
    fn t194_opt_i64_none_binds_null() {
        let v = opt_i64_to_d1(None).expect("None always succeeds");
        assert!(v.is_null());
    }

    #[wasm_bindgen_test]
    fn t194_opt_i64_some_converts_like_i64_to_d1() {
        let v = opt_i64_to_d1(Some(42)).expect("42 is in range");
        assert_eq!(v.as_f64(), Some(42.0));
    }

    #[wasm_bindgen_test]
    fn t194_opt_i64_some_out_of_range_is_still_rejected() {
        let too_big = (1i64 << 53) + 1;
        assert!(opt_i64_to_d1(Some(too_big)).is_err());
    }
}

/// Regression guard for `docs/src/d1-type-boundary.md` (subject 07d).
///
/// That document is a snapshot of another system's (D1's, and
/// `wasm-bindgen`'s) behaviour, confirmed against the local D1
/// runtime once. It will rot silently unless something checks it on
/// every build. These tests assert the load-bearing assumptions the
/// document -- and `bool_from_d1`/`i64_to_d1` -- are built on, so a
/// future `worker` or `wasm-bindgen` version that changes any of them
/// fails a test here rather than surfacing as a live defect months
/// later. If one of these ever fails, the document needs updating,
/// not the test.
#[cfg(test)]
mod d1_boundary_regression_tests {
    use wasm_bindgen_test::*;

    wasm_bindgen_test_configure!(run_in_node_experimental);

    /// The entire reason `i64_to_d1` exists: `wasm-bindgen` converts a
    /// raw `i64` to a JS `BigInt`, which is what D1's bind validation
    /// refuses (G-38). If a future `wasm-bindgen` routed `i64` through
    /// `JsValue::from_f64` instead (a JS Number) -- the same path
    /// `i32` and smaller already take -- this would fail, and it
    /// would mean `i64_to_d1`'s `BigInt`-avoidance strategy is now
    /// solving a problem that no longer exists (and worth
    /// simplifying, not just re-confirming).
    #[wasm_bindgen_test]
    fn a_raw_i64_still_converts_to_a_js_bigint() {
        let v = wasm_bindgen::JsValue::from(12345_i64);
        assert_eq!(
            v.js_typeof().as_string().as_deref(),
            Some("bigint"),
            "wasm-bindgen no longer routes a raw i64 through BigInt -- \
             if this changed, i64_to_d1's reason for existing changed with it"
        );
    }

    /// The wire shape `bool_from_d1` is built to accept: an `INTEGER`
    /// column arrives as a JS Number, never a JS Boolean (G-36).
    /// Constructed via `JSON.parse`, the same route D1's own JS
    /// values take, rather than fabricated Rust-side.
    #[wasm_bindgen_test]
    fn an_integer_column_value_arrives_as_a_number_not_a_boolean() {
        let value = js_sys::JSON::parse("1").expect("fixture JSON must itself be valid");
        assert_eq!(
            value.js_typeof().as_string().as_deref(),
            Some("number"),
            "D1's INTEGER columns no longer arrive as a JS number -- \
             bool_from_d1's whole visitor design assumes this"
        );
    }

    /// `NULL` deserializes to `None` for any `Option<T>` -- confirmed
    /// against the real local D1 runtime during subject 07d (every
    /// column in a scratch row read back `null`), pinned here so a
    /// future `serde_wasm_bindgen` version can't silently change it.
    #[wasm_bindgen_test]
    fn null_deserializes_to_none() {
        let value = js_sys::JSON::parse("null").expect("fixture JSON must itself be valid");
        let result: Option<i64> =
            serde_wasm_bindgen::from_value(value).expect("null must deserialize into Option::None");
        assert_eq!(result, None);
    }
}
