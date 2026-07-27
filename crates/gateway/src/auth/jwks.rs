//! JWKS (JSON Web Key Set) fetcher with KV caching.
//!
//! Fetches the public keys required to verify ID Token signatures from the IdP, and
//! caches them to reduce load on repeated verification.

use serde::{Deserialize, Serialize};
use worker::*;

const CACHE_KEY_PREFIX: &str = "jwks:";
const CACHE_TTL_SEC: u64 = 3600; // 1 時間

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Jwks {
    pub keys: Vec<serde_json::Value>,
}

/// JWKS を取得する。
///
/// Returns the cached entry from KV if present; otherwise fetches from the JWKS URI.
/// The URL is used directly as the cache key (we don't expect multiple IdPs in one instance).
pub async fn fetch(env: &Env, jwks_uri: &str) -> Result<Jwks> {
    let cache_key = format!("{}{}", CACHE_KEY_PREFIX, jwks_uri);

    // 1. Look up the KV cache
    if let Ok(kv) = env.kv("CACHE_KV") {
        if let Ok(Some(cached)) = kv.get(&cache_key).text().await {
            if let Ok(jwks) = serde_json::from_str::<Jwks>(&cached) {
                return Ok(jwks);
            }
        }
    }

    // 2. Fetch from the JWKS URI
    let mut init = RequestInit::new();
    init.with_method(Method::Get);
    let request = Request::new_with_init(jwks_uri, &init)?;
    let mut response = Fetch::Request(request).send().await?;

    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "JWKS fetch failed: status {}",
            response.status_code()
        )));
    }

    let body = response.text().await?;
    let jwks: Jwks = serde_json::from_str(&body)
        .map_err(|e| Error::RustError(format!("JWKS parse error: {}", e)))?;

    // 3. Write back to KV with a TTL
    if let Ok(kv) = env.kv("CACHE_KV") {
        let _ = kv
            .put(&cache_key, &body)
            .and_then(|b| Ok(b.expiration_ttl(CACHE_TTL_SEC)))
            .map(|b| b.execute());
    }

    Ok(jwks)
}

/// Find the JWK in the JWKS that matches the given kid.
///
/// If no kid is supplied (JWT header has no kid), the first key is returned.
/// When a kid is supplied, exact match is required (this matters during key rotation).
pub fn find_key<'a>(jwks: &'a Jwks, kid: Option<&str>) -> Option<&'a serde_json::Value> {
    match kid {
        Some(wanted) => jwks.keys.iter().find(|k| {
            k.get("kid").and_then(|v| v.as_str()) == Some(wanted)
        }),
        None => jwks.keys.first(),
    }
}
