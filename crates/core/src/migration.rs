//! Pure validation for migration payloads.
//!
//! Before any D1 work happens, the import handler runs `validate` on the
//! incoming payload. This catches schema mismatches, dangling foreign
//! references, and impossible field values up front so the operator gets a
//! single readable error rather than a half-applied import.
//!
//! Pure: no D1, no Worker types, just structural inspection of the payload.

use noye_shared::{MIGRATION_SCHEMA_VERSION, MigrationExport};
use std::collections::HashSet;

/// Result of validating an incoming import payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationResult {
    Ok { warnings: Vec<String> },
    Failed { errors: Vec<String> },
}

impl ValidationResult {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok { .. })
    }
    pub fn into_warnings(self) -> Vec<String> {
        match self {
            Self::Ok { warnings } => warnings,
            Self::Failed { .. } => Vec::new(),
        }
    }
    pub fn into_errors(self) -> Vec<String> {
        match self {
            Self::Failed { errors } => errors,
            Self::Ok { .. } => Vec::new(),
        }
    }
}

/// Validate an export payload prior to import.
///
/// Returns `Ok` with a possibly-empty list of non-fatal warnings, or
/// `Failed` with the first batch of fatal errors discovered. We collect
/// errors rather than bail on the first one so an operator gets the full
/// picture of what's wrong with their payload in one round-trip.
pub fn validate(payload: &MigrationExport) -> ValidationResult {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // ── Schema version ──
    if payload.schema_version != MIGRATION_SCHEMA_VERSION {
        errors.push(format!(
            "schema_version mismatch: payload is v{}, this server understands v{}",
            payload.schema_version, MIGRATION_SCHEMA_VERSION
        ));
        // No point checking the rest if the schema is wrong — the field
        // shapes might not even match.
        return ValidationResult::Failed { errors };
    }

    let data = &payload.data;

    // ── Identifier uniqueness within the payload ──
    let target_ids: HashSet<&str> = data.targets.iter().map(|t| t.id.as_str()).collect();
    if target_ids.len() != data.targets.len() {
        errors.push("targets contains duplicate IDs".to_string());
    }
    let channel_ids: HashSet<&str> = data.channels.iter().map(|c| c.id.as_str()).collect();
    if channel_ids.len() != data.channels.len() {
        errors.push("channels contains duplicate IDs".to_string());
    }
    let maintenance_ids: HashSet<&str> = data
        .maintenance_windows
        .iter()
        .map(|m| m.id.as_str())
        .collect();
    if maintenance_ids.len() != data.maintenance_windows.len() {
        errors.push("maintenance_windows contains duplicate IDs".to_string());
    }
    if let Some(users) = &data.users {
        let user_ids: HashSet<&str> = users.iter().map(|u| u.id.as_str()).collect();
        if user_ids.len() != users.len() {
            errors.push("users contains duplicate IDs".to_string());
        }
    }

    // ── Referential integrity for the join table ──
    let mut seen_link_pairs: HashSet<(&str, &str)> = HashSet::new();
    for link in &data.target_notifications {
        if !target_ids.contains(link.target_id.as_str()) {
            errors.push(format!(
                "target_notifications references unknown target_id {}",
                link.target_id
            ));
        }
        if !channel_ids.contains(link.channel_id.as_str()) {
            errors.push(format!(
                "target_notifications references unknown channel_id {}",
                link.channel_id
            ));
        }
        let pair = (link.target_id.as_str(), link.channel_id.as_str());
        if !seen_link_pairs.insert(pair) {
            errors.push(format!(
                "target_notifications contains duplicate ({}, {}) pair",
                link.target_id, link.channel_id
            ));
        }
    }

    // ── Owner integrity for targets ──
    // Only meaningful when users are exported. Without users, the owner_id
    // values are opaque strings that the destination resolves on its own.
    if let Some(users) = &data.users {
        let user_ids: HashSet<&str> = users.iter().map(|u| u.id.as_str()).collect();
        for t in &data.targets {
            if !user_ids.contains(t.owner_id.as_str()) {
                warnings.push(format!(
                    "target {} owned by user {} which is not present in the user export",
                    t.id, t.owner_id
                ));
            }
        }
        for c in &data.channels {
            if !user_ids.contains(c.owner_id.as_str()) {
                warnings.push(format!(
                    "channel {} owned by user {} which is not present in the user export",
                    c.id, c.owner_id
                ));
            }
        }
    } else {
        warnings.push(
            "users are not included in this payload; ensure the destination already has user \
             rows whose IDs match the owner_id values referenced by targets and channels"
                .to_string(),
        );
    }

    // ── Per-record sanity ──
    for t in &data.targets {
        if t.id.is_empty() {
            errors.push("target with empty id is not allowed".to_string());
        }
        if t.name.trim().is_empty() {
            errors.push(format!("target {} has empty name", t.id));
        }
        if t.host.trim().is_empty() {
            errors.push(format!("target {} has empty host", t.id));
        }
    }
    for c in &data.channels {
        if c.id.is_empty() {
            errors.push("channel with empty id is not allowed".to_string());
        }
        if !["webhook", "slack", "email"].contains(&c.channel_type.as_str()) {
            errors.push(format!(
                "channel {} has unknown channel_type {}",
                c.id, c.channel_type
            ));
        }
    }

    if errors.is_empty() {
        ValidationResult::Ok { warnings }
    } else {
        ValidationResult::Failed { errors }
    }
}

/// Quick-glance row counts. Used by both the dry-run path (estimate) and the
/// real-import path (initial counts before conflict resolution).
pub fn count_rows(payload: &MigrationExport) -> (i64, i64, i64, i64, i64) {
    (
        payload.data.targets.len() as i64,
        payload.data.channels.len() as i64,
        payload.data.target_notifications.len() as i64,
        payload.data.maintenance_windows.len() as i64,
        payload
            .data
            .users
            .as_ref()
            .map(|u| u.len() as i64)
            .unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use noye_shared::*;

    fn user(id: &str) -> User {
        User {
            id: id.to_string(),
            email: format!("{}@example.com", id),
            name: id.to_string(),
            role: "member".to_string(),
            is_active: true,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn target(id: &str, owner: &str) -> Target {
        Target {
            id: id.to_string(),
            name: format!("target-{}", id),
            target_type: "https".to_string(),
            host: "example.com".to_string(),
            port: None,
            path: None,
            expected_status: Some(200),
            body_contains: None,
            tls_threshold_days: Some(30),
            timeout_sec: 10,
            retry_count: 3,
            interval_minutes: 5,
            is_disabled: false,
            owner_id: owner.to_string(),
            tags: None,
            next_check_at: "2026-04-01T00:00:00Z".to_string(),
            created_at: "2026-04-01T00:00:00Z".to_string(),
            updated_at: "2026-04-01T00:00:00Z".to_string(),
        }
    }

    fn channel(id: &str, owner: &str) -> NotificationChannel {
        NotificationChannel {
            id: id.to_string(),
            name: format!("channel-{}", id),
            channel_type: "webhook".to_string(),
            endpoint: "https://hooks.example.com/x".to_string(),
            is_enabled: true,
            owner_id: owner.to_string(),
            created_at: "2026-04-01T00:00:00Z".to_string(),
        }
    }

    fn link(target_id: &str, channel_id: &str) -> TargetNotificationLink {
        TargetNotificationLink {
            target_id: target_id.to_string(),
            channel_id: channel_id.to_string(),
            on_down: true,
            on_up: false,
        }
    }

    fn payload(data: MigrationData) -> MigrationExport {
        MigrationExport {
            schema_version: MIGRATION_SCHEMA_VERSION,
            exported_at: "2026-04-28T00:00:00Z".to_string(),
            source_deployment: Some("test".to_string()),
            data,
        }
    }

    fn empty_data() -> MigrationData {
        MigrationData {
            targets: Vec::new(),
            channels: Vec::new(),
            target_notifications: Vec::new(),
            maintenance_windows: Vec::new(),
            users: None,
        }
    }

    #[test]
    fn empty_payload_validates_with_a_warning() {
        let p = payload(empty_data());
        let r = validate(&p);
        assert!(r.is_ok());
        // The "users not included" warning should fire even on an empty payload
        // because users are absent (None).
        let warnings = r.into_warnings();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("users are not included"))
        );
    }

    #[test]
    fn schema_version_mismatch_fails_immediately() {
        let mut p = payload(empty_data());
        p.schema_version = 999;
        let r = validate(&p);
        assert!(!r.is_ok());
        let errs = r.into_errors();
        assert!(errs.iter().any(|e| e.contains("schema_version")));
    }

    #[test]
    fn duplicate_target_ids_fail() {
        let p = payload(MigrationData {
            targets: vec![target("t1", "u1"), target("t1", "u1")],
            ..empty_data()
        });
        let r = validate(&p);
        let errs = r.into_errors();
        assert!(
            errs.iter()
                .any(|e| e.contains("targets contains duplicate"))
        );
    }

    #[test]
    fn duplicate_channel_ids_fail() {
        let p = payload(MigrationData {
            channels: vec![channel("c1", "u1"), channel("c1", "u1")],
            ..empty_data()
        });
        let r = validate(&p);
        assert!(
            r.into_errors()
                .iter()
                .any(|e| e.contains("channels contains duplicate"))
        );
    }

    #[test]
    fn link_referencing_unknown_target_fails() {
        let p = payload(MigrationData {
            channels: vec![channel("c1", "u1")],
            target_notifications: vec![link("ghost", "c1")],
            ..empty_data()
        });
        let r = validate(&p);
        assert!(
            r.into_errors()
                .iter()
                .any(|e| e.contains("unknown target_id ghost"))
        );
    }

    #[test]
    fn link_referencing_unknown_channel_fails() {
        let p = payload(MigrationData {
            targets: vec![target("t1", "u1")],
            target_notifications: vec![link("t1", "ghost")],
            ..empty_data()
        });
        let r = validate(&p);
        assert!(
            r.into_errors()
                .iter()
                .any(|e| e.contains("unknown channel_id ghost"))
        );
    }

    #[test]
    fn duplicate_link_pair_fails() {
        let p = payload(MigrationData {
            targets: vec![target("t1", "u1")],
            channels: vec![channel("c1", "u1")],
            target_notifications: vec![link("t1", "c1"), link("t1", "c1")],
            ..empty_data()
        });
        let r = validate(&p);
        assert!(
            r.into_errors()
                .iter()
                .any(|e| e.contains("duplicate (t1, c1)"))
        );
    }

    #[test]
    fn unknown_owner_warns_when_users_present() {
        let p = payload(MigrationData {
            targets: vec![target("t1", "stranger")],
            users: Some(vec![user("u1")]),
            ..empty_data()
        });
        let r = validate(&p);
        assert!(r.is_ok());
        let warnings = r.into_warnings();
        assert!(warnings.iter().any(|w| w.contains("stranger")));
    }

    #[test]
    fn unknown_owner_does_not_warn_when_users_absent() {
        // When users are not included, owner_id values are expected to be
        // resolved against the destination's user table — so we don't warn.
        let p = payload(MigrationData {
            targets: vec![target("t1", "u1")],
            users: None, // explicit
            ..empty_data()
        });
        let r = validate(&p);
        assert!(r.is_ok());
        // Should still see the umbrella "users are not included" warning, but
        // not a per-target stranger warning.
        let warnings = r.into_warnings();
        assert!(
            warnings
                .iter()
                .any(|w| w.contains("users are not included"))
        );
        assert!(!warnings.iter().any(|w| w.contains("owned by user")));
    }

    #[test]
    fn empty_target_id_is_rejected() {
        let p = payload(MigrationData {
            targets: vec![target("", "u1")],
            ..empty_data()
        });
        let r = validate(&p);
        assert!(r.into_errors().iter().any(|e| e.contains("empty id")));
    }

    #[test]
    fn unknown_channel_type_is_rejected() {
        let mut bad = channel("c1", "u1");
        bad.channel_type = "carrier-pigeon".to_string();
        let p = payload(MigrationData {
            channels: vec![bad],
            ..empty_data()
        });
        let r = validate(&p);
        assert!(r.into_errors().iter().any(|e| e.contains("carrier-pigeon")));
    }

    #[test]
    fn count_rows_returns_each_table_size() {
        let p = payload(MigrationData {
            targets: vec![target("t1", "u1"), target("t2", "u1")],
            channels: vec![channel("c1", "u1")],
            target_notifications: vec![link("t1", "c1"), link("t2", "c1")],
            maintenance_windows: vec![],
            users: Some(vec![user("u1")]),
        });
        let counts = count_rows(&p);
        assert_eq!(counts, (2, 1, 2, 0, 1));
    }

    #[test]
    fn count_rows_returns_zero_for_absent_users() {
        let p = payload(empty_data());
        let counts = count_rows(&p);
        assert_eq!(counts.4, 0);
    }

    #[test]
    fn validation_errors_accumulate_rather_than_short_circuit() {
        // Multiple unrelated problems — operator should see all of them.
        let mut bad_channel = channel("c1", "u1");
        bad_channel.channel_type = "carrier-pigeon".to_string();
        let p = payload(MigrationData {
            targets: vec![target("", "u1")],
            channels: vec![bad_channel],
            target_notifications: vec![link("ghost", "also-ghost")],
            maintenance_windows: vec![],
            users: None,
        });
        let r = validate(&p);
        let errs = r.into_errors();
        assert!(
            errs.len() >= 3,
            "should report at least 3 errors, got {:?}",
            errs
        );
    }

    #[test]
    fn well_formed_payload_with_users_validates_clean() {
        let p = payload(MigrationData {
            targets: vec![target("t1", "u1")],
            channels: vec![channel("c1", "u1")],
            target_notifications: vec![link("t1", "c1")],
            maintenance_windows: vec![],
            users: Some(vec![user("u1")]),
        });
        let r = validate(&p);
        assert!(r.is_ok());
        assert!(r.into_warnings().is_empty());
    }

    #[test]
    fn import_conflict_policy_default_is_skip() {
        let policy: ImportConflictPolicy = Default::default();
        assert_eq!(policy, ImportConflictPolicy::Skip);
    }

    #[test]
    fn import_conflict_policy_serializes_lowercase() {
        let s = serde_json::to_string(&ImportConflictPolicy::Replace).unwrap();
        assert_eq!(s, "\"replace\"");
        let parsed: ImportConflictPolicy = serde_json::from_str("\"fail\"").unwrap();
        assert_eq!(parsed, ImportConflictPolicy::Fail);
    }
}
