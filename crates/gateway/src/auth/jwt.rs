//! JWT (RFC 7519) parsing and verification.
//!
//! Signature verification is delegated to the Web Crypto API (see `crypto::verify_jwt_signature`).
//! Claim validation (iss, aud, exp, nonce) is owned by this module.

use serde::{Deserialize, Serialize};
use worker::*;

use super::crypto;
use super::jwks;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtHeader {
    pub alg: String,
    #[serde(default)]
    pub kid: Option<String>,
    #[serde(default, rename = "typ")]
    pub typ: Option<String>,
}

/// Claims of an OIDC ID Token (OIDC Core 1.0 §2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer identifier
    pub iss: String,
    /// Subject identifier (unique within the IdP)
    pub sub: String,
    /// Audience (client ID)
    #[serde(default)]
    pub aud: AudClaim,
    /// Expiry (Unix timestamp)
    pub exp: i64,
    /// Issued-at timestamp
    #[serde(default)]
    pub iat: Option<i64>,
    /// Nonce (must match the value sent in the authorization request)
    #[serde(default)]
    pub nonce: Option<String>,
    /// Email (when scope=email is granted)
    #[serde(default)]
    pub email: Option<String>,
    /// Email-verified flag
    #[serde(default)]
    pub email_verified: Option<bool>,
    /// Display name (when scope=profile is granted)
    #[serde(default)]
    pub name: Option<String>,
    /// Remaining claims are kept untouched
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// The `aud` claim may be a string or an array.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AudClaim {
    Single(String),
    Multiple(Vec<String>),
}

impl Default for AudClaim {
    fn default() -> Self {
        AudClaim::Single(String::new())
    }
}

impl AudClaim {
    pub fn contains(&self, expected: &str) -> bool {
        match self {
            AudClaim::Single(s) => s == expected,
            AudClaim::Multiple(v) => v.iter().any(|s| s == expected),
        }
    }
}

/// Verification rules.
pub struct Verification<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    /// Expected nonce (the value persisted with the session state)
    pub expected_nonce: Option<&'a str>,
    /// Clock skew tolerance, in seconds
    pub leeway_sec: i64,
}

/// Verify the ID Tokenしてクレームを返す。
///
/// # Validation steps
/// 1. JWT structure (header.payload.signature)
/// 2. Signature (the JWK is picked by kid from the JWKS, then verified via Web Crypto for the alg)
/// 3. `iss` matches the expected issuer
/// 4. `aud` contains the configured client_id
/// 5. `exp` is in the future (with leeway)
/// 6. `nonce` matches when one is configured
pub async fn verify_id_token(
    env: &Env,
    token: &str,
    jwks_uri: &str,
    verification: &Verification<'_>,
) -> Result<IdTokenClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(Error::RustError(
            "Malformed JWT: expected 3 segments".to_string(),
        ));
    }

    // 1. Decode the header and payload
    let header_bytes = crypto::base64url_decode(parts[0])
        .map_err(|e| Error::RustError(format!("Header decode error: {}", e)))?;
    let header: JwtHeader = serde_json::from_slice(&header_bytes)
        .map_err(|e| Error::RustError(format!("Header parse error: {}", e)))?;

    let payload_bytes = crypto::base64url_decode(parts[1])
        .map_err(|e| Error::RustError(format!("Payload decode error: {}", e)))?;
    let claims: IdTokenClaims = serde_json::from_slice(&payload_bytes)
        .map_err(|e| Error::RustError(format!("Payload parse error: {}", e)))?;

    let signature = crypto::base64url_decode(parts[2])
        .map_err(|e| Error::RustError(format!("Signature decode error: {}", e)))?;

    // 2. Select the key from the JWKS and verify the signature
    let jwks = jwks::fetch(env, jwks_uri).await?;
    let key = jwks::find_key(&jwks, header.kid.as_deref()).ok_or_else(|| {
        Error::RustError(format!("No matching JWK found for kid={:?}", header.kid))
    })?;

    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let verified =
        crypto::verify_jwt_signature(key, &header.alg, signing_input.as_bytes(), &signature)
            .await
            .map_err(|e| Error::RustError(format!("Signature verification error: {}", e)))?;

    if !verified {
        return Err(Error::RustError("JWT signature invalid".to_string()));
    }

    // 3. Claim validation
    if claims.iss != verification.issuer {
        return Err(Error::RustError(format!(
            "iss mismatch: got {}, expected {}",
            claims.iss, verification.issuer
        )));
    }

    if !claims.aud.contains(verification.audience) {
        return Err(Error::RustError(format!(
            "aud does not contain expected audience: {}",
            verification.audience
        )));
    }

    let now = chrono::Utc::now().timestamp();
    if claims.exp + verification.leeway_sec < now {
        return Err(Error::RustError(format!(
            "JWT expired: exp={}, now={}",
            claims.exp, now
        )));
    }

    if let Some(expected_nonce) = verification.expected_nonce {
        match &claims.nonce {
            Some(got) if got == expected_nonce => {}
            Some(got) => {
                return Err(Error::RustError(format!(
                    "nonce mismatch: got {}, expected {}",
                    got, expected_nonce
                )));
            }
            None => {
                return Err(Error::RustError(
                    "nonce claim missing from ID Token".to_string(),
                ));
            }
        }
    }

    Ok(claims)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aud_single_matches_exact_string() {
        let aud = AudClaim::Single("client-123".to_string());
        assert!(aud.contains("client-123"));
        assert!(!aud.contains("client-124"));
        assert!(!aud.contains(""));
    }

    #[test]
    fn aud_single_is_case_sensitive() {
        let aud = AudClaim::Single("client-A".to_string());
        assert!(aud.contains("client-A"));
        assert!(!aud.contains("client-a"));
    }

    #[test]
    fn aud_multiple_matches_any_element() {
        let aud = AudClaim::Multiple(vec![
            "client-1".to_string(),
            "client-2".to_string(),
            "client-3".to_string(),
        ]);
        assert!(aud.contains("client-1"));
        assert!(aud.contains("client-2"));
        assert!(aud.contains("client-3"));
        assert!(!aud.contains("client-4"));
    }

    #[test]
    fn aud_multiple_empty_never_matches() {
        let aud = AudClaim::Multiple(vec![]);
        assert!(!aud.contains("anything"));
        assert!(!aud.contains(""));
    }

    #[test]
    fn aud_default_is_empty_single() {
        let aud = AudClaim::default();
        // The default is Single("") and only matches the empty string
        assert!(aud.contains(""));
        assert!(!aud.contains("client-1"));
    }

    #[test]
    fn aud_deserializes_from_string_form() {
        // OIDC IdPs commonly serialize "aud" as a single string
        let json = r#""client-abc""#;
        let aud: AudClaim = serde_json::from_str(json).expect("deserialize string aud");
        assert!(aud.contains("client-abc"));
        assert!(!aud.contains("client-xyz"));
    }

    #[test]
    fn aud_deserializes_from_array_form() {
        // RFC 7519 allows "aud" to be an array
        let json = r#"["client-abc", "client-xyz"]"#;
        let aud: AudClaim = serde_json::from_str(json).expect("deserialize array aud");
        assert!(aud.contains("client-abc"));
        assert!(aud.contains("client-xyz"));
        assert!(!aud.contains("client-other"));
    }
}
