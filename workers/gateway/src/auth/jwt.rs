//! JWT (RFC 7519) のパースと検証。
//!
//! 署名検証は Web Crypto API 経由で行う (`crypto::verify_jwt_signature`)。
//! クレーム検証 (iss, aud, exp, nonce) はこのモジュールで責任を持つ。

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

/// OIDC ID Token のクレーム (OIDC Core 1.0 §2)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdTokenClaims {
    /// Issuer Identifier
    pub iss: String,
    /// Subject Identifier (IdP 内での一意な ID)
    pub sub: String,
    /// Audience (Client ID)
    #[serde(default)]
    pub aud: AudClaim,
    /// 有効期限 (Unix time)
    pub exp: i64,
    /// 発行時刻
    #[serde(default)]
    pub iat: Option<i64>,
    /// nonce (認可リクエスト時の値と一致すべき)
    #[serde(default)]
    pub nonce: Option<String>,
    /// Email (scope=email の場合)
    #[serde(default)]
    pub email: Option<String>,
    /// Email verified フラグ
    #[serde(default)]
    pub email_verified: Option<bool>,
    /// Display name (scope=profile の場合)
    #[serde(default)]
    pub name: Option<String>,
    /// 残りのクレームは未解釈のまま保持
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

/// `aud` クレームは文字列または配列。
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

/// 検証ルール。
pub struct Verification<'a> {
    pub issuer: &'a str,
    pub audience: &'a str,
    /// 期待する nonce (セッション state に紐づけて保存された値)
    pub expected_nonce: Option<&'a str>,
    /// 時計スキュー許容 (秒)
    pub leeway_sec: i64,
}

/// ID Token を検証してクレームを返す。
///
/// # 検証項目
/// 1. JWT 構造 (header.payload.signature) の整合性
/// 2. 署名検証 (JWKS から kid で鍵を選択し、alg に応じた Web Crypto 呼び出し)
/// 3. iss が期待値と一致
/// 4. aud に期待の client_id が含まれる
/// 5. exp が未来 (leeway を考慮)
/// 6. nonce が一致 (指定時のみ)
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

    // 1. Header と Payload をデコード
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

    // 2. JWKS から鍵を選択して署名検証
    let jwks = jwks::fetch(env, jwks_uri).await?;
    let key = jwks::find_key(&jwks, header.kid.as_deref()).ok_or_else(|| {
        Error::RustError(format!(
            "No matching JWK found for kid={:?}",
            header.kid
        ))
    })?;

    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let verified = crypto::verify_jwt_signature(
        key,
        &header.alg,
        signing_input.as_bytes(),
        &signature,
    )
    .await
    .map_err(|e| Error::RustError(format!("Signature verification error: {}", e)))?;

    if !verified {
        return Err(Error::RustError("JWT signature invalid".to_string()));
    }

    // 3. クレーム検証
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
