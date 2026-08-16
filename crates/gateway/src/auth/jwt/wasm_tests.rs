//! T-102 (subject 20, G-19): a token signed by an unknown key is
//! rejected under each of the three override configurations. Sibling
//! module per PRQ-05, named distinctly from `jwt.rs`'s own inline
//! `mod tests` (a `mod tests;` here would collide with it by name).
//!
//! `verify_id_token` itself needs a live `Env` and an HTTP fetch to
//! `jwks::fetch` -- nothing a `wasm-bindgen-test` can mock in this
//! project. `verify_id_token_with_jwks` (the same function, minus the
//! fetch) is what's actually under test here: it's exactly where
//! `resolve_token_and_jwks` (the function `oidc.rs`'s host tests cover
//! in `auth/oidc/tests.rs`) hands off, so together the two test files
//! cover the whole claim -- the right JWKS source is selected per
//! configuration (host-tested, no I/O), and whatever JWKS ends up
//! there, a wrong-key token is rejected by real Web Crypto (wasm-
//! tested here). The configurations themselves don't change what this
//! function receives or does; each test below documents that the
//! property holds under a *named* one, rather than asserting it once
//! and leaving the other two configurations unchecked by anything
//! literal in the test suite.
//!
//! Fixture: a real RS256 keypair and a real signature, generated with
//! Node's built-in `crypto` module (`crypto.generateKeyPairSync` /
//! `crypto.createSign`) and independently verified with
//! `crypto.verify` before being embedded here -- not hand-written.
//! `correct_jwk` is the public key that actually signed `TOKEN`;
//! `wrong_jwk` is an unrelated keypair's public key, standing in for
//! "the key an attacker's forged token was signed with does not
//! appear in the real JWKS."

use super::super::jwks::Jwks;
use super::{IdTokenClaims, Verification, verify_id_token_with_jwks};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_node_experimental);

const TOKEN: &str = "eyJhbGciOiJSUzI1NiIsImtpZCI6InRlc3Qta2V5LTEiLCJ0eXAiOiJKV1QifQ.eyJpc3MiOiJodHRwczovL2lkcC5leGFtcGxlLmNvbSIsInN1YiI6InVzZXItc3ViLTEyMyIsImF1ZCI6InRlc3QtY2xpZW50LWlkIiwiZXhwIjoxOTAwMDAzNjAwLCJpYXQiOjE5MDAwMDAwMDAsIm5vbmNlIjoidGVzdC1ub25jZS1hYmMiLCJlbWFpbCI6InBlcnNvbkBleGFtcGxlLmNvbSJ9.ne4XPyE2k-reYkuTsY3LgaIO4mrzW4VOkPiCsTMJLOjsNkbZ3fePO3VopTQMC15PoNKZQHn_YStDoCq_FQNTiOIQH5M3u6ryukI_mEclrgu84-nkngpNu9mDQozRjcd3TMw8C9rfbE1aaBJ0Wb5tfnCd7BQ7t1oRESfLrMMBIClkcApFu7_7OtlaprZnDNdbl9b9YyPE5skO-FEv3V2uGSvcgYQU0GHM3SStoB48vW3oj47IgY-RFWPtAZvoxbALGbX-xYYDzls8LiC5sDmw8naxlPz-LFR-0bpoRHyGlUs9whfYWrxUNdXqWGXEO3OwilxcuI4FrkBhjVXkqzEMkw";

fn correct_jwk() -> serde_json::Value {
    serde_json::json!({
        "kty": "RSA",
        "n": "ws0GSkBAHYeW0PxDKHHjUn0vw1zsSwkgJsPELx0YO0xa2JktCBl0HbGpXjdwRPUoftJvctGUgiXNJlVslX55FeKHHWezL9HaBLPy8lQ2mhmyeGebtMQRmurkrRB8kqV1wmRRfQ9WDVLhvJQ6ZXnDQuA7F5hbc3fnP3PX1HH6JXbgOuRqrxZ8LCKfSopVjnUanukD7RY_7feR71K4Vc7RqTiFNJbHMXRUFMtdzBqkn3P1EDAxMnbhw_Q6nNVYy07P8qMbPB0lEWV71kg7bkNzDsBUjIG9RcbPYzvCk8rluKP3NRLX65aFppXLskeIzdxWWv7fHLOBjopiVKVTkMpllQ",
        "e": "AQAB",
        "alg": "RS256",
        "kid": "test-key-1",
        "use": "sig"
    })
}

fn wrong_jwk() -> serde_json::Value {
    // Deliberately carries the SAME kid as the token's own header
    // ("test-key-1"). If it didn't, find_key would reject the token
    // on a kid mismatch before signature verification ever runs --
    // caught directly, by disabling the signature check itself and
    // confirming these tests still failed for the wrong reason (kid
    // lookup, not Web Crypto) before fixing this. The realistic
    // threat T-102 guards against is a JWKS source that legitimately
    // returns *a* key under this kid, just not the one that actually
    // signed the token.
    serde_json::json!({
        "kty": "RSA",
        "n": "z-vCYQ5bZwH49BfDbfrUbCyZAacHQrtMmFIU9AMgBOf_Y-_GIv5VUeCUcutG35l_FoGTCQ5fPftwU_k6yd8HSUQ44oTNy4N0uMyrU-ysurnokVswgxDfrtVdZ4D1rXOXHU1nen7mEDHsdlxyWhcnnNEdQVAIOyf3R9s0_WE_Zrb5eIRU8YiVQBKcHhlxIq5wmPq5QlOwBnJFOq_cYFCATmhCzSa2d5ob5d_HZC-LlKaiskq9H-1W_Kbk2I5zzfCeBo9sH_ydkf8Ac0KxXUAm_6CQAarVKz0qrOf17o5N4cqtRYw-BVC2WDPmWecZzDyM0HZpHgv3-wPAqcCC1uDtvw",
        "e": "AQAB",
        "alg": "RS256",
        "kid": "test-key-1"
    })
}

fn verification() -> Verification<'static> {
    Verification {
        issuer: "https://idp.example.com",
        audience: "test-client-id",
        expected_nonce: Some("test-nonce-abc"),
        leeway_sec: 60,
    }
}

fn assert_claims(claims: &IdTokenClaims) {
    assert_eq!(claims.sub, "user-sub-123");
    assert_eq!(claims.iss, "https://idp.example.com");
}

#[wasm_bindgen_test]
async fn sanity_the_correct_key_verifies() {
    // Proves the fixture and the harness are sound -- without this,
    // the three rejection tests below could pass for the wrong reason
    // (a verifier that rejects everything).
    let jwks = Jwks {
        keys: vec![correct_jwk()],
    };
    let claims = verify_id_token_with_jwks(&jwks, TOKEN, &verification())
        .await
        .expect("the token's own key must verify it");
    assert_claims(&claims);
}

#[wasm_bindgen_test]
async fn t102_all_overrides_configuration_rejects_an_unknown_key() {
    // Stands in for: OIDC_AUTH_URL, OIDC_TOKEN_URL and OIDC_JWKS_URL
    // all set -- resolve_token_and_jwks (auth/oidc.rs) returns the
    // operator's configured jwks_url_override untouched, skipping
    // discovery. Whatever JWKS that URL served did not contain the key
    // that actually signed this token.
    let jwks = Jwks {
        keys: vec![wrong_jwk()],
    };
    let result = verify_id_token_with_jwks(&jwks, TOKEN, &verification()).await;
    assert!(
        result.is_err(),
        "a token signed by an unknown key was accepted under the all-overrides configuration"
    );
}

#[wasm_bindgen_test]
async fn t102_no_overrides_configuration_rejects_an_unknown_key() {
    // Stands in for: none of the three set -- resolve_token_and_jwks
    // falls through entirely to Discovery, exactly as before subject
    // 20. Discovery served a JWKS that did not contain the key that
    // actually signed this token.
    let jwks = Jwks {
        keys: vec![wrong_jwk()],
    };
    let result = verify_id_token_with_jwks(&jwks, TOKEN, &verification()).await;
    assert!(
        result.is_err(),
        "a token signed by an unknown key was accepted under the no-overrides configuration"
    );
}

#[wasm_bindgen_test]
async fn t102_partial_override_configuration_rejects_an_unknown_key() {
    // Stands in for: e.g. only OIDC_JWKS_URL set -- resolve_token_and_
    // jwks takes the jwks_url_override directly and still discovers
    // token_endpoint. Whichever JWKS the override served did not
    // contain the key that actually signed this token.
    let jwks = Jwks {
        keys: vec![wrong_jwk()],
    };
    let result = verify_id_token_with_jwks(&jwks, TOKEN, &verification()).await;
    assert!(
        result.is_err(),
        "a token signed by an unknown key was accepted under a partial-override configuration"
    );
}

#[wasm_bindgen_test]
async fn t102_an_empty_jwks_is_also_rejected_under_each_configuration() {
    // The degenerate case of "the resolved JWKS source didn't have
    // this key" -- no keys at all, e.g. a misconfigured override URL
    // returning an empty set. find_key must fail closed, not fall
    // back to trusting an unsigned/unverified claim.
    let jwks = Jwks { keys: vec![] };
    let result = verify_id_token_with_jwks(&jwks, TOKEN, &verification()).await;
    assert!(result.is_err(), "an empty JWKS accepted a token");
}
