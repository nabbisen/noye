//! Tests for `env_check.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md` for the standing rule
//! against adding inline `#[cfg(test)] mod tests` blocks.

use super::*;

// ── Environment::parse ──

#[test]
fn parse_development_variants() {
    assert_eq!(Environment::parse("development"), Environment::Development);
    assert_eq!(Environment::parse("Development"), Environment::Development);
    assert_eq!(Environment::parse("DEVELOPMENT"), Environment::Development);
}

#[test]
fn parse_production_when_unset_or_unknown() {
    assert_eq!(Environment::parse(""), Environment::Production);
    assert_eq!(Environment::parse("production"), Environment::Production);
    assert_eq!(Environment::parse("staging"), Environment::Production);
    assert_eq!(Environment::parse("dev"), Environment::Production); // strict: not "development"
    assert_eq!(Environment::parse("test"), Environment::Production);
}

#[test]
fn is_development_helper() {
    assert!(Environment::Development.is_development());
    assert!(!Environment::Production.is_development());
}

// ── KNOWN_DEV_FALLBACKS ──

#[test]
fn known_dev_fallbacks_includes_oidc_secret() {
    let names: Vec<&str> = KNOWN_DEV_FALLBACKS.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"OIDC_CLIENT_SECRET"));
}

#[test]
fn known_dev_fallbacks_includes_gateway_shared_token() {
    let names: Vec<&str> = KNOWN_DEV_FALLBACKS.iter().map(|(n, _)| *n).collect();
    assert!(names.contains(&"GATEWAY_SHARED_TOKEN"));
}

#[test]
fn known_dev_fallbacks_match_documented_values() {
    // These literals are duplicated in crates/gateway/wrangler.toml.example's
    // instructional comments. If you change either one, change the other.
    let oidc = KNOWN_DEV_FALLBACKS
        .iter()
        .find(|(n, _)| *n == "OIDC_CLIENT_SECRET")
        .map(|(_, v)| *v)
        .unwrap();
    assert_eq!(oidc, "dev-idp-does-not-verify-this");

    let token = KNOWN_DEV_FALLBACKS
        .iter()
        .find(|(n, _)| *n == "GATEWAY_SHARED_TOKEN")
        .map(|(_, v)| *v)
        .unwrap();
    assert_eq!(token, "noye-local-dev-shared-token");
}

// ── find_leaked_fallback (T-11 regression) ──

#[test]
fn leaked_value_is_refused() {
    let observed = [
        (
            "OIDC_CLIENT_SECRET",
            Some("dev-idp-does-not-verify-this".to_string()),
        ),
        (
            "GATEWAY_SHARED_TOKEN",
            Some("noye-local-dev-shared-token".to_string()),
        ),
    ];
    assert!(find_leaked_fallback(&observed).is_err());
}

#[test]
fn leaked_value_is_refused_with_no_development_bypass() {
    // This is T-11 exactly: before 2026-07-28, an early return on
    // `is_development()` skipped this check entirely when NOYE_ENV was
    // "development" — which is what the shipped wrangler.toml set, so the
    // control could never fire. `find_leaked_fallback` takes no
    // environment parameter at all now: there is no branch left that
    // could reintroduce the bypass.
    let observed = [(
        "GATEWAY_SHARED_TOKEN",
        Some("noye-local-dev-shared-token".to_string()),
    )];
    let err = find_leaked_fallback(&observed).unwrap_err();
    assert!(err.contains("GATEWAY_SHARED_TOKEN"));
}

#[test]
fn non_denylisted_value_is_accepted() {
    let observed = [(
        "GATEWAY_SHARED_TOKEN",
        Some("a-real-generated-secret".to_string()),
    )];
    assert!(find_leaked_fallback(&observed).is_ok());
}

#[test]
fn unset_variable_is_accepted() {
    let observed = [("GATEWAY_SHARED_TOKEN", None)];
    assert!(find_leaked_fallback(&observed).is_ok());
}

#[test]
fn no_observations_at_all_is_accepted() {
    assert!(find_leaked_fallback(&[]).is_ok());
}

#[test]
fn error_names_the_variable_but_not_the_value() {
    let observed = [(
        "GATEWAY_SHARED_TOKEN",
        Some("noye-local-dev-shared-token".to_string()),
    )];
    let err = find_leaked_fallback(&observed).unwrap_err();
    assert!(err.contains("GATEWAY_SHARED_TOKEN"));
    assert!(!err.contains("noye-local-dev-shared-token"));
}

#[test]
fn only_the_leaked_variable_is_named_when_only_one_leaks() {
    let observed = [
        ("OIDC_CLIENT_SECRET", Some("a-real-secret".to_string())),
        (
            "GATEWAY_SHARED_TOKEN",
            Some("noye-local-dev-shared-token".to_string()),
        ),
    ];
    let err = find_leaked_fallback(&observed).unwrap_err();
    assert!(err.contains("GATEWAY_SHARED_TOKEN"));
    assert!(!err.contains("OIDC_CLIENT_SECRET"));
}
