//! Tests for `auth/oidc.rs`'s discovery-skip decision (subject 20,
//! G-19). Sibling module per PRQ-05 — see
//! `rfcs/handoffs/33-test-module-migration.md`.
//!
//! Pure functions, no Worker runtime needed: `discover`, `build_
//! authorization_request` and `handle_callback` all take `&Env` and
//! perform real HTTP fetches, so they can't run as host tests. The
//! *decision* of whether those fetches happen at all, and which value
//! wins when they don't, is pulled out into `auth_endpoint_needs_
//! discovery`, `token_and_jwks_need_discovery` and `apply_token_and_
//! jwks_overrides` specifically so it can be tested without one.
//! T-102 (forged-token rejection under each configuration) needs real
//! JWKS signature verification through Web Crypto and belongs in the
//! wasm suite instead — see `crates/gateway/src/auth/jwt/tests.rs` (or
//! wherever the wasm suite for this module lives).

use super::*;

fn some(url: &str) -> Option<String> {
    Some(url.to_string())
}

// ── T-99: with all three overrides set, no discovery request is made ──

#[test]
fn t99_all_three_overrides_set_needs_no_discovery_for_auth_endpoint() {
    assert!(!auth_endpoint_needs_discovery(&some(
        "https://idp.example.com/auth"
    )));
}

#[test]
fn t99_all_three_overrides_set_needs_no_discovery_for_token_and_jwks() {
    assert!(!token_and_jwks_need_discovery(
        &some("https://idp.example.com/token"),
        &some("https://idp.example.com/jwks"),
    ));
}

// ── T-100: with none set, behaviour is unchanged (discovery is used) ──

#[test]
fn t100_none_set_auth_endpoint_still_needs_discovery() {
    assert!(auth_endpoint_needs_discovery(&None));
}

#[test]
fn t100_none_set_token_and_jwks_still_need_discovery() {
    assert!(token_and_jwks_need_discovery(&None, &None));
}

#[test]
fn t100_none_set_discovered_values_pass_through_unchanged() {
    let (token, jwks) = apply_token_and_jwks_overrides(
        &None,
        &None,
        "https://discovered.example.com/token",
        "https://discovered.example.com/jwks",
    );
    assert_eq!(token, "https://discovered.example.com/token");
    assert_eq!(jwks, "https://discovered.example.com/jwks");
}

// ── T-101: with some set, the remainder still come from discovery ──

#[test]
fn t101_auth_url_alone_needs_no_discovery_but_others_still_do() {
    // Setting only the auth override means build_authorization_request
    // skips discovery entirely -- it never reads token or jwks fields --
    // but token_and_jwks resolution, an independent call, is unaffected
    // by it and still needs discovery.
    assert!(!auth_endpoint_needs_discovery(&some(
        "https://idp.example.com/auth"
    )));
    assert!(token_and_jwks_need_discovery(&None, &None));
}

#[test]
fn t101_only_token_url_set_still_needs_discovery_for_jwks() {
    assert!(token_and_jwks_need_discovery(
        &some("https://idp.example.com/token"),
        &None,
    ));
}

#[test]
fn t101_only_token_url_set_the_override_wins_and_jwks_comes_from_discovery() {
    let (token, jwks) = apply_token_and_jwks_overrides(
        &some("https://override.example.com/token"),
        &None,
        "https://discovered.example.com/token",
        "https://discovered.example.com/jwks",
    );
    assert_eq!(token, "https://override.example.com/token");
    assert_eq!(jwks, "https://discovered.example.com/jwks");
}

#[test]
fn t101_only_jwks_url_set_the_override_wins_and_token_comes_from_discovery() {
    let (token, jwks) = apply_token_and_jwks_overrides(
        &None,
        &some("https://override.example.com/jwks"),
        "https://discovered.example.com/token",
        "https://discovered.example.com/jwks",
    );
    assert_eq!(token, "https://discovered.example.com/token");
    assert_eq!(jwks, "https://override.example.com/jwks");
}

#[test]
fn t101_both_token_and_jwks_set_needs_no_discovery() {
    assert!(!token_and_jwks_need_discovery(
        &some("https://override.example.com/token"),
        &some("https://override.example.com/jwks"),
    ));
}
