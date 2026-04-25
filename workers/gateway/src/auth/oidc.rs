//! 汎用 OIDC クライアント。
//!
//! OpenID Connect Core 1.0 仕様の最小サブセットを実装する:
//! - Authorization Code Flow (response_type=code)
//! - PKCE (RFC 7636, S256)
//! - Discovery (OIDC Discovery 1.0)
//! - state + nonce による CSRF/replay 対策
//!
//! 特定の IDaaS に依存しない設計。`OIDC_ISSUER_URL` の変更のみで
//! Google / Okta / Auth0 / Microsoft Entra ID / Keycloak / AWS Cognito
//! など任意のプロバイダに対応する。

use serde::{Deserialize, Serialize};
use worker::*;

use super::crypto;
use super::jwt;
use super::session::{self, PendingLogin};

const DISCOVERY_SUFFIX: &str = "/.well-known/openid-configuration";
const DISCOVERY_CACHE_KEY: &str = "oidc:discovery";
const DISCOVERY_CACHE_TTL_SEC: u64 = 3600;

/// OIDC Provider Metadata (OIDC Discovery 1.0 §3)。
/// 本実装で必要な項目のみ取り出し、それ以外は破棄する。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Discovery {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
    #[serde(default)]
    pub end_session_endpoint: Option<String>,
    #[serde(default)]
    pub userinfo_endpoint: Option<String>,
    #[serde(default)]
    pub scopes_supported: Option<Vec<String>>,
}

/// 環境変数から OIDC 設定を読み込む。
pub struct Config {
    pub issuer_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub scopes: String,
}

impl Config {
    pub fn from_env(env: &Env) -> Result<Self> {
        let issuer_url = env
            .var("OIDC_ISSUER_URL")
            .map_err(|_| Error::RustError("OIDC_ISSUER_URL not set".to_string()))?
            .to_string();
        let client_id = env
            .var("OIDC_CLIENT_ID")
            .map_err(|_| Error::RustError("OIDC_CLIENT_ID not set".to_string()))?
            .to_string();
        // client_secret は機密なので secret として登録される想定
        let client_secret = env
            .secret("OIDC_CLIENT_SECRET")
            .map(|v| v.to_string())
            .or_else(|_| env.var("OIDC_CLIENT_SECRET").map(|v| v.to_string()))
            .map_err(|_| Error::RustError("OIDC_CLIENT_SECRET not set".to_string()))?;
        let redirect_uri = env
            .var("OIDC_REDIRECT_URI")
            .map_err(|_| Error::RustError("OIDC_REDIRECT_URI not set".to_string()))?
            .to_string();
        let scopes = env
            .var("OIDC_SCOPES")
            .map(|v| v.to_string())
            .unwrap_or_else(|_| "openid email profile".to_string());

        Ok(Self {
            issuer_url,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
        })
    }
}

/// Discovery ドキュメントを取得する (KV キャッシュ付き)。
pub async fn discover(env: &Env, issuer_url: &str) -> Result<Discovery> {
    // issuer_url ごとにキャッシュキーを分離
    let cache_key = format!("{}:{}", DISCOVERY_CACHE_KEY, issuer_url);

    if let Ok(kv) = env.kv("CACHE_KV") {
        if let Ok(Some(cached)) = kv.get(&cache_key).text().await {
            if let Ok(d) = serde_json::from_str::<Discovery>(&cached) {
                return Ok(d);
            }
        }
    }

    let url = if issuer_url.ends_with('/') {
        format!("{}{}", &issuer_url[..issuer_url.len() - 1], DISCOVERY_SUFFIX)
    } else {
        format!("{}{}", issuer_url, DISCOVERY_SUFFIX)
    };

    let mut init = RequestInit::new();
    init.with_method(Method::Get);
    let request = Request::new_with_init(&url, &init)?;
    let mut response = Fetch::Request(request).send().await?;

    if response.status_code() < 200 || response.status_code() >= 300 {
        return Err(Error::RustError(format!(
            "Discovery fetch failed: {} status {}",
            url,
            response.status_code()
        )));
    }

    let body = response.text().await?;
    let discovery: Discovery = serde_json::from_str(&body)
        .map_err(|e| Error::RustError(format!("Discovery parse error: {}", e)))?;

    // issuer 値の一致確認 (OIDC Discovery 1.0 §4.3 の推奨)
    // 完全一致でない環境もあるため warn 扱いでスキップ
    if discovery.issuer != issuer_url && discovery.issuer.trim_end_matches('/') != issuer_url.trim_end_matches('/') {
        console_warn!(
            "Discovery issuer mismatch: document={}, configured={}",
            discovery.issuer,
            issuer_url
        );
    }

    if let Ok(kv) = env.kv("CACHE_KV") {
        let _ = kv
            .put(&cache_key, &body)
            .and_then(|b| Ok(b.expiration_ttl(DISCOVERY_CACHE_TTL_SEC)))
            .map(|b| b.execute());
    }

    Ok(discovery)
}

/// 認可リクエスト URL を構築する。
///
/// PKCE verifier / state / nonce を生成し、`return_to` とともに KV に保存する。
/// ユーザーを返された URL にリダイレクトすると、IdP が `/auth/callback` に
/// code と state を付けて戻してくる。
pub async fn build_authorization_request(
    env: &Env,
    return_to: &str,
) -> Result<String> {
    let config = Config::from_env(env)?;
    let discovery = discover(env, &config.issuer_url).await?;

    // state, nonce, pkce_verifier を生成
    let state_bytes = crypto::random_bytes(32)
        .map_err(|e| Error::RustError(format!("rng error: {}", e)))?;
    let state = crypto::base64url_encode(&state_bytes);

    let nonce_bytes = crypto::random_bytes(32)
        .map_err(|e| Error::RustError(format!("rng error: {}", e)))?;
    let nonce = crypto::base64url_encode(&nonce_bytes);

    let verifier_bytes = crypto::random_bytes(32)
        .map_err(|e| Error::RustError(format!("rng error: {}", e)))?;
    let pkce_verifier = crypto::base64url_encode(&verifier_bytes);

    // PKCE challenge = BASE64URL(SHA256(verifier))
    let challenge_digest = crypto::sha256(pkce_verifier.as_bytes())
        .await
        .map_err(|e| Error::RustError(format!("sha256 error: {}", e)))?;
    let code_challenge = crypto::base64url_encode(&challenge_digest);

    // ペンディングステートを KV に保存
    let pending = PendingLogin {
        state: state.clone(),
        nonce: nonce.clone(),
        pkce_verifier,
        return_to: return_to.to_string(),
        created_at: chrono::Utc::now().timestamp(),
    };
    session::save_pending(env, &pending).await?;

    // 認可リクエスト URL 組み立て
    let mut url = discovery.authorization_endpoint.clone();
    url.push_str(if url.contains('?') { "&" } else { "?" });
    url.push_str(&format!("response_type=code"));
    url.push_str(&format!("&client_id={}", urlencoding::encode(&config.client_id)));
    url.push_str(&format!(
        "&redirect_uri={}",
        urlencoding::encode(&config.redirect_uri)
    ));
    url.push_str(&format!("&scope={}", urlencoding::encode(&config.scopes)));
    url.push_str(&format!("&state={}", urlencoding::encode(&state)));
    url.push_str(&format!("&nonce={}", urlencoding::encode(&nonce)));
    url.push_str(&format!(
        "&code_challenge={}",
        urlencoding::encode(&code_challenge)
    ));
    url.push_str("&code_challenge_method=S256");

    Ok(url)
}

/// コールバックから受け取った code を ID Token に交換し、検証する。
pub async fn handle_callback(
    env: &Env,
    code: &str,
    state: &str,
) -> Result<(jwt::IdTokenClaims, String)> {
    let config = Config::from_env(env)?;
    let discovery = discover(env, &config.issuer_url).await?;

    // 1. state から pending login を取り出す (CSRF 対策)
    let pending = session::consume_pending(env, state).await?.ok_or_else(|| {
        Error::RustError("Invalid or expired state parameter".to_string())
    })?;

    // 2. token endpoint に POST
    let body = format!(
        "grant_type=authorization_code&code={}&redirect_uri={}&client_id={}&client_secret={}&code_verifier={}",
        urlencoding::encode(code),
        urlencoding::encode(&config.redirect_uri),
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&config.client_secret),
        urlencoding::encode(&pending.pkce_verifier),
    );

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;
    headers.set("Accept", "application/json")?;
    init.with_headers(headers);
    init.with_body(Some(wasm_bindgen::JsValue::from_str(&body)));

    let request = Request::new_with_init(&discovery.token_endpoint, &init)?;
    let mut response = Fetch::Request(request).send().await?;

    let status = response.status_code();
    let response_body = response.text().await?;

    if status < 200 || status >= 300 {
        return Err(Error::RustError(format!(
            "Token endpoint error: status {} body: {}",
            status, response_body
        )));
    }

    let token_response: TokenResponse = serde_json::from_str(&response_body)
        .map_err(|e| Error::RustError(format!("Token response parse error: {}", e)))?;

    let id_token = token_response.id_token.ok_or_else(|| {
        Error::RustError("Token response did not include id_token".to_string())
    })?;

    // 3. ID Token を検証
    let verification = jwt::Verification {
        issuer: &discovery.issuer,
        audience: &config.client_id,
        expected_nonce: Some(&pending.nonce),
        leeway_sec: 60,
    };
    let claims =
        jwt::verify_id_token(env, &id_token, &discovery.jwks_uri, &verification).await?;

    Ok((claims, pending.return_to))
}

/// ログアウト時のリダイレクト先 URL を返す (end_session_endpoint が広告されていれば)。
pub async fn end_session_url(env: &Env) -> Result<Option<String>> {
    let config = Config::from_env(env)?;
    let discovery = discover(env, &config.issuer_url).await?;
    Ok(discovery.end_session_endpoint)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    #[serde(default)]
    id_token: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    access_token: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    token_type: Option<String>,
    #[allow(dead_code)]
    #[serde(default)]
    expires_in: Option<i64>,
    #[allow(dead_code)]
    #[serde(default)]
    refresh_token: Option<String>,
}
