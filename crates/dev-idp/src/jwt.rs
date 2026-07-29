//! RS256 JWT signing for ID Tokens.
//!
//! Produces tokens compatible with the OIDC Core 1.0 §2 specification.
//! Verifier compatibility tested manually against the noye gateway's
//! `auth::crypto::jwt_verify::verify_jwt_signature` (which uses Web
//! Crypto under WASM).

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::pkcs1v15::SigningKey;
use rsa::sha2::Sha256;
use rsa::signature::{RandomizedSigner, SignatureEncoding};
use serde_json::{Value, json};

use crate::keys::KeyMaterial;

pub struct IdTokenClaims<'a> {
    /// The OIDC issuer URL (must match the IdP's advertised `issuer`).
    pub issuer: &'a str,
    /// Subject — stable identifier for the authenticated user.
    pub sub: &'a str,
    /// Audience — the OIDC client_id.
    pub audience: &'a str,
    /// `nonce` echoed from the authorization request.
    pub nonce: &'a str,
    /// User email (claimed by Noye's gateway after token verification).
    pub email: &'a str,
    /// Display name (informational).
    pub name: &'a str,
    /// Token lifetime in seconds from now.
    pub lifetime_sec: i64,
}

/// Build a signed RS256 JWT from claims and private key material.
pub fn sign_id_token(km: &KeyMaterial, claims: &IdTokenClaims) -> anyhow::Result<String> {
    let now = chrono::Utc::now().timestamp();
    let header = json!({
        "alg": "RS256",
        "typ": "JWT",
        "kid": km.kid,
    });
    let payload = json!({
        "iss": claims.issuer,
        "sub": claims.sub,
        "aud": claims.audience,
        "exp": now + claims.lifetime_sec,
        "iat": now,
        "nonce": claims.nonce,
        "email": claims.email,
        "email_verified": true,
        "name": claims.name,
    });

    let signing_input = format!(
        "{}.{}",
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header)?),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload)?)
    );

    let signing_key = SigningKey::<Sha256>::new(km.private.clone());
    let mut rng = rand::thread_rng();
    let signature = signing_key.sign_with_rng(&mut rng, signing_input.as_bytes());

    Ok(format!(
        "{}.{}",
        signing_input,
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    ))
}

/// Helper: parse a JWT into header/payload (without verifying the signature).
/// Used by tests in this crate, not by production verifier code.
#[allow(dead_code)]
pub fn parse_unverified(jwt: &str) -> anyhow::Result<(Value, Value)> {
    let parts: Vec<&str> = jwt.split('.').collect();
    if parts.len() != 3 {
        anyhow::bail!("malformed JWT: expected 3 parts, got {}", parts.len());
    }
    let header_bytes = URL_SAFE_NO_PAD.decode(parts[0])?;
    let payload_bytes = URL_SAFE_NO_PAD.decode(parts[1])?;
    Ok((
        serde_json::from_slice(&header_bytes)?,
        serde_json::from_slice(&payload_bytes)?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_token_has_three_dot_separated_parts() {
        let km = KeyMaterial::fresh().unwrap();
        let claims = IdTokenClaims {
            issuer: "http://localhost:5556",
            sub: "local-admin-1",
            audience: "noye-local-client",
            nonce: "test-nonce",
            email: "admin@local.test",
            name: "Local Admin",
            lifetime_sec: 600,
        };
        let token = sign_id_token(&km, &claims).unwrap();
        assert_eq!(token.matches('.').count(), 2);
    }

    #[test]
    fn signed_token_carries_expected_claims() {
        let km = KeyMaterial::fresh().unwrap();
        let claims = IdTokenClaims {
            issuer: "http://localhost:5556",
            sub: "local-admin-1",
            audience: "noye-local-client",
            nonce: "abc-123",
            email: "admin@local.test",
            name: "Local Admin",
            lifetime_sec: 600,
        };
        let token = sign_id_token(&km, &claims).unwrap();
        let (header, payload) = parse_unverified(&token).unwrap();
        assert_eq!(header["alg"], "RS256");
        assert_eq!(header["typ"], "JWT");
        assert_eq!(header["kid"], km.kid);
        assert_eq!(payload["iss"], "http://localhost:5556");
        assert_eq!(payload["sub"], "local-admin-1");
        assert_eq!(payload["aud"], "noye-local-client");
        assert_eq!(payload["nonce"], "abc-123");
        assert_eq!(payload["email"], "admin@local.test");
        assert_eq!(payload["email_verified"], true);
    }

    #[test]
    fn signed_token_exp_is_future() {
        let km = KeyMaterial::fresh().unwrap();
        let now = chrono::Utc::now().timestamp();
        let claims = IdTokenClaims {
            issuer: "http://localhost:5556",
            sub: "local-admin-1",
            audience: "noye-local-client",
            nonce: "x",
            email: "admin@local.test",
            name: "Local Admin",
            lifetime_sec: 600,
        };
        let token = sign_id_token(&km, &claims).unwrap();
        let (_, payload) = parse_unverified(&token).unwrap();
        let exp = payload["exp"].as_i64().unwrap();
        assert!(exp >= now + 590); // some leeway for slow CI
        assert!(exp <= now + 610);
    }
}
