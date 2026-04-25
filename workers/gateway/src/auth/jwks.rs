//! JWKS (JSON Web Key Set) の取得と KV キャッシュ。
//!
//! ID Token の署名検証に必要な公開鍵を IdP から取得し、
//! キャッシュで繰り返し検証時の負荷を抑制する。

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
/// KV に TTL 付きでキャッシュされていればそれを返し、なければ JWKS URI から fetch する。
/// キャッシュキーは URL をそのまま使用 (1インスタンスで複数 IdP が来る想定はしていない)。
pub async fn fetch(env: &Env, jwks_uri: &str) -> Result<Jwks> {
    let cache_key = format!("{}{}", CACHE_KEY_PREFIX, jwks_uri);

    // 1. KV キャッシュヒット確認
    if let Ok(kv) = env.kv("CACHE_KV") {
        if let Ok(Some(cached)) = kv.get(&cache_key).text().await {
            if let Ok(jwks) = serde_json::from_str::<Jwks>(&cached) {
                return Ok(jwks);
            }
        }
    }

    // 2. JWKS URI から取得
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

    // 3. KV に書き戻し (TTL 付き)
    if let Ok(kv) = env.kv("CACHE_KV") {
        let _ = kv
            .put(&cache_key, &body)
            .and_then(|b| Ok(b.expiration_ttl(CACHE_TTL_SEC)))
            .map(|b| b.execute());
    }

    Ok(jwks)
}

/// kid に一致する JWK を JWKS から探す。
///
/// kid が指定されていない場合 (JWT header に kid が無い場合) は最初の鍵を返す。
/// 複数鍵ローテーション時の安全性のため、kid 指定がある場合は厳密一致を要求する。
pub fn find_key<'a>(jwks: &'a Jwks, kid: Option<&str>) -> Option<&'a serde_json::Value> {
    match kid {
        Some(wanted) => jwks.keys.iter().find(|k| {
            k.get("kid").and_then(|v| v.as_str()) == Some(wanted)
        }),
        None => jwks.keys.first(),
    }
}
