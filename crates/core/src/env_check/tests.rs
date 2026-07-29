//! Tests for `env_check.rs`. Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md` for the standing rule
//! against adding inline `#[cfg(test)] mod tests` blocks.

use super::*;

#[test]
fn known_dev_fallback_has_gateway_shared_token() {
    let names: Vec<&str> = KNOWN_DEV_FALLBACKS.iter().map(|(n, _)| *n).collect();
    assert_eq!(names, vec!["GATEWAY_SHARED_TOKEN"]);
}

#[test]
fn known_dev_fallback_value_matches_documented_value() {
    let token = KNOWN_DEV_FALLBACKS
        .iter()
        .find(|(n, _)| *n == "GATEWAY_SHARED_TOKEN")
        .map(|(_, v)| *v)
        .unwrap();
    assert_eq!(token, "noye-local-dev-shared-token");
}

// ── find_leaked_fallback (T-11 regression) ──

#[test]
fn leaked_value_is_refused_with_no_development_bypass() {
    // T-11: before 2026-07-28, an early return on `is_development()`
    // skipped this check when NOYE_ENV was "development" — which is what
    // the shipped wrangler.toml set, so the control never fired.
    // `find_leaked_fallback` takes no environment parameter now: there is
    // no branch left that could reintroduce the bypass.
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
fn error_names_the_variable_but_not_the_value() {
    let observed = [(
        "GATEWAY_SHARED_TOKEN",
        Some("noye-local-dev-shared-token".to_string()),
    )];
    let err = find_leaked_fallback(&observed).unwrap_err();
    assert!(err.contains("GATEWAY_SHARED_TOKEN"));
    assert!(!err.contains("noye-local-dev-shared-token"));
}
