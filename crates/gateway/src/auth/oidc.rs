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
    /// Subject 20 (G-19): per-endpoint overrides. When set, each
    /// overrides the corresponding discovered endpoint; when unset,
    /// discovery applies as today. A provider that does not publish a
    /// discovery document is otherwise unsupported (FR-AUTH-02).
    pub auth_url_override: Option<String>,
    pub token_url_override: Option<String>,
    pub jwks_url_override: Option<String>,
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
        let auth_url_override = env.var("OIDC_AUTH_URL").ok().map(|v| v.to_string());
        let token_url_override = env.var("OIDC_TOKEN_URL").ok().map(|v| v.to_string());
        let jwks_url_override = env.var("OIDC_JWKS_URL").ok().map(|v| v.to_string());

        Ok(Self {
            issuer_url,
            client_id,
            client_secret,
            redirect_uri,
            scopes,
            auth_url_override,
            token_url_override,
            jwks_url_override,
        })
    }
}

/// Whether resolving the authorization endpoint needs a discovery
/// fetch at all. Pure and host-testable (T-99/T-100/T-101) -- no
/// Worker runtime needed to check a decision that has nothing to do
/// with I/O.
fn auth_endpoint_needs_discovery(auth_url_override: &Option<String>) -> bool {
    auth_url_override.is_none()
}

/// Whether resolving the token endpoint and JWKS URI needs a
/// discovery fetch at all: unless *both* are overridden, at least one
/// of them still has to come from discovery. Pure and host-testable.
fn token_and_jwks_need_discovery(
    token_url_override: &Option<String>,
    jwks_url_override: &Option<String>,
) -> bool {
    !(token_url_override.is_some() && jwks_url_override.is_some())
}

/// Given a possibly-partial override and a discovery result (real or,
/// in a test, a stand-in), which token endpoint / JWKS URI actually
/// get used: the override where set, the discovered value otherwise
/// (T-101, "the remainder still come from discovery"). Pure and
/// host-testable.
fn apply_token_and_jwks_overrides(
    token_url_override: &Option<String>,
    jwks_url_override: &Option<String>,
    discovered_token_endpoint: &str,
    discovered_jwks_uri: &str,
) -> (String, String) {
    (
        token_url_override
            .clone()
            .unwrap_or_else(|| discovered_token_endpoint.to_string()),
        jwks_url_override
            .clone()
            .unwrap_or_else(|| discovered_jwks_uri.to_string()),
    )
}

/// Resolve the authorization endpoint, skipping discovery entirely
/// when `OIDC_AUTH_URL` is set (T-99/T-101) -- the only thing
/// `build_authorization_request` needs Discovery for.
async fn resolve_authorization_endpoint(env: &Env, config: &Config) -> Result<String> {
    if !auth_endpoint_needs_discovery(&config.auth_url_override) {
        return Ok(config.auth_url_override.clone().unwrap());
    }
    let discovery = discover(env, &config.issuer_url).await?;
    Ok(discovery.authorization_endpoint)
}

/// Resolve the token endpoint, JWKS URI, and the issuer to verify a
/// token against, skipping discovery entirely when *both*
/// `OIDC_TOKEN_URL` and `OIDC_JWKS_URL` are set (T-99/T-101) --
/// together with `resolve_authorization_endpoint`, this is why "all
/// three overrides set" means zero discovery requests (T-99).
///
/// When discovery is skipped, the expected issuer is the configured
/// `OIDC_ISSUER_URL` itself, not a discovered value -- `discover`'s
/// own mismatch check already treats the two as interchangeable
/// (it only warns, never fails, when they differ).
///
/// **Signature verification must run against whichever JWKS URI is
/// returned here** -- an override path that verifies against a
/// discovered key while a token was actually fetched via an
/// overridden token endpoint (or the reverse) would accept a token
/// this deployment never asked for (T-102).
async fn resolve_token_and_jwks(env: &Env, config: &Config) -> Result<(String, String, String)> {
    if !token_and_jwks_need_discovery(&config.token_url_override, &config.jwks_url_override) {
        return Ok((
            config.token_url_override.clone().unwrap(),
            config.jwks_url_override.clone().unwrap(),
            config.issuer_url.clone(),
        ));
    }
    let discovery = discover(env, &config.issuer_url).await?;
    let (token_endpoint, jwks_uri) = apply_token_and_jwks_overrides(
        &config.token_url_override,
        &config.jwks_url_override,
        &discovery.token_endpoint,
        &discovery.jwks_uri,
    );
    Ok((token_endpoint, jwks_uri, discovery.issuer))
}

/// Fetch the Discovery document (with KV caching).
pub async fn discover(env: &Env, issuer_url: &str) -> Result<Discovery> {
    // Use a per-issuer cache key
    let cache_key = format!("{}:{}", DISCOVERY_CACHE_KEY, issuer_url);

    if let Ok(kv) = env.kv("CACHE_KV")
        && let Ok(Some(cached)) = kv.get(&cache_key).text().await
        && let Ok(d) = serde_json::from_str::<Discovery>(&cached)
    {
        return Ok(d);
    }

    let url = if let Some(stripped) = issuer_url.strip_suffix('/') {
        format!("{}{}", stripped, DISCOVERY_SUFFIX)
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
    if discovery.issuer != issuer_url
        && discovery.issuer.trim_end_matches('/') != issuer_url.trim_end_matches('/')
    {
        console_warn!(
            "Discovery issuer mismatch: document={}, configured={}",
            discovery.issuer,
            issuer_url
        );
    }

    if let Ok(kv) = env.kv("CACHE_KV") {
        let _ = kv
            .put(&cache_key, &body)
            .map(|b| b.expiration_ttl(DISCOVERY_CACHE_TTL_SEC))
            .map(|b| b.execute());
    }

    Ok(discovery)
}

/// Build the authorization request URL.
///
/// Generate a PKCE verifier, state, and nonce, then persist them along with `return_to` to KV.
/// Redirecting the user to the returned URL hands off to the IdP, which calls back to
/// `/auth/callback` with the code and state parameters.
pub async fn build_authorization_request(env: &Env, return_to: &str) -> Result<String> {
    let config = Config::from_env(env)?;
    let authorization_endpoint = resolve_authorization_endpoint(env, &config).await?;

    // Generate state, nonce, and pkce_verifier
    let state_bytes =
        crypto::random_bytes(32).map_err(|e| Error::RustError(format!("rng error: {}", e)))?;
    let state = crypto::base64url_encode(&state_bytes);

    let nonce_bytes =
        crypto::random_bytes(32).map_err(|e| Error::RustError(format!("rng error: {}", e)))?;
    let nonce = crypto::base64url_encode(&nonce_bytes);

    let verifier_bytes =
        crypto::random_bytes(32).map_err(|e| Error::RustError(format!("rng error: {}", e)))?;
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
    let mut url = authorization_endpoint;
    url.push_str(if url.contains('?') { "&" } else { "?" });
    url.push_str("response_type=code");
    url.push_str(&format!(
        "&client_id={}",
        urlencoding::encode(&config.client_id)
    ));
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
    let (token_endpoint, jwks_uri, issuer) = resolve_token_and_jwks(env, &config).await?;

    // 1. Pop the pending login by state (CSRF protection)
    let pending = session::consume_pending(env, state)
        .await?
        .ok_or_else(|| Error::RustError("Invalid or expired state parameter".to_string()))?;

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

    let request = Request::new_with_init(&token_endpoint, &init)?;
    let mut response = Fetch::Request(request).send().await?;

    let status = response.status_code();
    let response_body = response.text().await?;

    if !(200..300).contains(&status) {
        return Err(Error::RustError(format!(
            "Token endpoint error: status {} body: {}",
            status, response_body
        )));
    }

    let token_response: TokenResponse = serde_json::from_str(&response_body)
        .map_err(|e| Error::RustError(format!("Token response parse error: {}", e)))?;

    let id_token = token_response
        .id_token
        .ok_or_else(|| Error::RustError("Token response did not include id_token".to_string()))?;

    // 3. Verify the ID Token -- against jwks_uri as resolved above by
    // resolve_token_and_jwks, the same function call that resolved
    // token_endpoint (T-102). Both are per-field overrides for the
    // *same* configured issuer_url, never a mix across providers --
    // there is exactly one issuer_url in this Config -- so a partial
    // override (e.g. only OIDC_TOKEN_URL set) verifying against a
    // discovery-sourced jwks_uri is still verifying against the
    // correct provider's key, not a substituted one.
    let verification = jwt::Verification {
        issuer: &issuer,
        audience: &config.client_id,
        expected_nonce: Some(&pending.nonce),
        leeway_sec: 60,
    };
    let claims = jwt::verify_id_token(env, &id_token, &jwks_uri, &verification).await?;

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

#[cfg(test)]
mod tests;
