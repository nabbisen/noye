//! Generic OIDC client.
//!
//! Implements a minimal subset of OpenID Connect Core 1.0:
//! - Authorization Code Flow (response_type=code)
//! - PKCE (RFC 7636, S256)
//! - Discovery (OIDC Discovery 1.0)
//! - state + nonce による CSRF/replay 対策
//!
//! Provider-agnostic by design: changing only `OIDC_ISSUER_URL` is enough to switch between
//! Google / Okta / Auth0 / Microsoft Entra ID / Keycloak / AWS Cognito
//! and any other compliant provider.

use serde::{Deserialize, Serialize};
use worker::*;

use super::crypto;
use super::jwt;
use super::session::{self, PendingLogin};

const DISCOVERY_SUFFIX: &str = "/.well-known/openid-configuration";
const DISCOVERY_CACHE_KEY: &str = "oidc:discovery";
const DISCOVERY_CACHE_TTL_SEC: u64 = 3600;

/// OIDC Provider Metadata (OIDC Discovery 1.0 §3).
/// Only the fields used by this implementation are kept; the rest are discarded.
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

/// Load OIDC configuration from environment variables.
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
        // client_secret is sensitive and is expected to be registered as a Wrangler secret
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

/// Fetch the Discovery document (with KV caching).
pub async fn discover(env: &Env, issuer_url: &str) -> Result<Discovery> {
    // Use a per-issuer cache key
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

    // Verify that the issuer matches (per OIDC Discovery 1.0 §4.3 recommendation)
    // Some environments do not return an exact match, so we only warn and proceed
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

/// Build the authorization request URL.
///
/// Generate a PKCE verifier, state, and nonce, then persist them along with `return_to` to KV.
/// Redirecting the user to the returned URL hands off to the IdP, which calls back to
/// `/auth/callback` with the code and state parameters.
pub async fn build_authorization_request(
    env: &Env,
    return_to: &str,
) -> Result<String> {
    let config = Config::from_env(env)?;
    let discovery = discover(env, &config.issuer_url).await?;

    // Generate state, nonce, and pkce_verifier
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

    // Persist the pending login state to KV
    let pending = PendingLogin {
        state: state.clone(),
        nonce: nonce.clone(),
        pkce_verifier,
        return_to: return_to.to_string(),
        created_at: chrono::Utc::now().timestamp(),
    };
    session::save_pending(env, &pending).await?;

    // Compose the authorization request URL
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

/// Exchange the code received in the callback for an ID Token and verify it.
pub async fn handle_callback(
    env: &Env,
    code: &str,
    state: &str,
) -> Result<(jwt::IdTokenClaims, String)> {
    let config = Config::from_env(env)?;
    let discovery = discover(env, &config.issuer_url).await?;

    // 1. Pop the pending login by state (CSRF protection)
    let pending = session::consume_pending(env, state).await?.ok_or_else(|| {
        Error::RustError("Invalid or expired state parameter".to_string())
    })?;

    // 2. POST to the token endpoint
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
    let headers = Headers::new();
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

    // 3. Verify the ID Token
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

/// Return the post-logout redirect URL (if the IdP advertises end_session_endpoint).
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
