//! OIDC endpoint handlers.
//!
//! Each handler does the minimum needed to keep the noye gateway's OIDC
//! flow happy. Where OIDC permits options (e.g. PKCE methods, JWKS
//! algorithms), we hard-code the choice that mirrors what we know the
//! gateway uses, rather than supporting every permutation.

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use crate::jwt::{IdTokenClaims, sign_id_token};
use crate::state::PendingCode;

type BoxBody = Full<Bytes>;

/// Top-level dispatch. Matches on (method, path) and routes to the
/// concrete handler. Anything unknown gets a 404 with a hint.
pub async fn dispatch(
    req: Request<Incoming>,
    state: Arc<AppState>,
) -> Result<Response<BoxBody>, hyper::Error> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    log_request(&method, &path);

    let result = match (method.clone(), path.as_str()) {
        (Method::GET, "/.well-known/openid-configuration") => discovery(&state),
        (Method::GET, "/jwks") => jwks(&state),
        (Method::GET, "/authorize") => authorize(req, &state).await,
        (Method::POST, "/token") => token(req, &state).await,
        (Method::GET, "/healthz") => Ok(json_response(StatusCode::OK, &json!({"ok": true}))),
        _ => Ok(json_response(
            StatusCode::NOT_FOUND,
            &json!({
                "error": "not_found",
                "method": method.as_str(),
                "path": path,
                "hint": "noye-dev-idp serves only /authorize, /token, /jwks, /.well-known/openid-configuration",
            }),
        )),
    };

    match result {
        Ok(resp) => Ok(resp),
        Err(err) => {
            eprintln!("[dev-idp] handler error on {} {}: {}", method, path, err);
            Ok(json_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &json!({"error": "internal", "message": err.to_string()}),
            ))
        }
    }
}

fn log_request(method: &Method, path: &str) {
    eprintln!("[dev-idp] {} {}", method, path);
}

// ── Discovery ──

fn discovery(state: &AppState) -> anyhow::Result<Response<BoxBody>> {
    let issuer = &state.config.issuer;
    let body = json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{}/authorize", issuer),
        "token_endpoint": format!("{}/token", issuer),
        "jwks_uri": format!("{}/jwks", issuer),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "scopes_supported": ["openid", "email", "profile"],
        "token_endpoint_auth_methods_supported": ["client_secret_post"],
        "claims_supported": ["sub", "email", "email_verified", "name", "iss", "aud", "exp", "iat", "nonce"],
        "code_challenge_methods_supported": ["plain", "S256"],
    });
    Ok(json_response(StatusCode::OK, &body))
}

// ── JWKS ──

fn jwks(state: &AppState) -> anyhow::Result<Response<BoxBody>> {
    Ok(json_response(StatusCode::OK, &state.keys.to_jwks()))
}

// ── Authorize ──

async fn authorize(req: Request<Incoming>, state: &AppState) -> anyhow::Result<Response<BoxBody>> {
    let query = req.uri().query().unwrap_or("");
    let params = parse_query(query);

    // Required parameters per OIDC Authorization Code Flow.
    let response_type = params
        .get("response_type")
        .map(String::as_str)
        .unwrap_or("");
    if response_type != "code" {
        return Ok(error_redirect_or_400(
            params.get("redirect_uri"),
            params.get("state"),
            "unsupported_response_type",
            &format!("only 'code' is supported, got '{}'", response_type),
        ));
    }

    let client_id = match params.get("client_id") {
        Some(c) if !c.is_empty() => c.clone(),
        _ => {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                &json!({"error": "missing client_id"}),
            ));
        }
    };
    if client_id != state.config.client_id {
        return Ok(json_response(
            StatusCode::BAD_REQUEST,
            &json!({
                "error": "unknown client_id",
                "received": client_id,
                "expected": state.config.client_id,
            }),
        ));
    }

    let redirect_uri = match params.get("redirect_uri") {
        Some(u) if !u.is_empty() => u.clone(),
        _ => {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                &json!({"error": "missing redirect_uri"}),
            ));
        }
    };

    let state_param = params.get("state").cloned().unwrap_or_default();
    let nonce = params.get("nonce").cloned().unwrap_or_default();
    let code_challenge = params.get("code_challenge").cloned();
    let code_challenge_method = params.get("code_challenge_method").cloned();

    // Mint a fresh code and stash the auth-time context.
    let code = Uuid::new_v4().to_string();
    state.codes.put(
        code.clone(),
        PendingCode {
            state: state_param.clone(),
            nonce,
            code_challenge,
            code_challenge_method,
            redirect_uri: redirect_uri.clone(),
            created_at: chrono::Utc::now().timestamp(),
        },
    );

    let location = format!(
        "{}{}code={}&state={}",
        redirect_uri,
        if redirect_uri.contains('?') { "&" } else { "?" },
        urlencoding::encode(&code),
        urlencoding::encode(&state_param),
    );

    Response::builder()
        .status(StatusCode::FOUND)
        .header("Location", location)
        .header("Cache-Control", "no-store")
        .body(Full::new(Bytes::new()))
        .map_err(Into::into)
}

// ── Token ──

async fn token(req: Request<Incoming>, state: &AppState) -> anyhow::Result<Response<BoxBody>> {
    let body_bytes = req.into_body().collect().await?.to_bytes();
    let body = std::str::from_utf8(&body_bytes).unwrap_or("");
    let params = parse_query(body);

    let grant_type = params.get("grant_type").map(String::as_str).unwrap_or("");
    if grant_type != "authorization_code" {
        return Ok(json_response(
            StatusCode::BAD_REQUEST,
            &json!({"error": "unsupported_grant_type", "grant_type": grant_type}),
        ));
    }

    let code = match params.get("code") {
        Some(c) if !c.is_empty() => c.clone(),
        _ => {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                &json!({"error": "missing code"}),
            ));
        }
    };

    let pending = match state.codes.consume(&code) {
        Some(p) => p,
        None => {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                &json!({
                    "error": "invalid_grant",
                    "error_description": "code unknown or expired",
                }),
            ));
        }
    };

    // PKCE verification (when the auth request had a challenge).
    if let Some(challenge) = &pending.code_challenge {
        let verifier = params.get("code_verifier").cloned().unwrap_or_default();
        let method = pending
            .code_challenge_method
            .clone()
            .unwrap_or_else(|| "plain".to_string());
        let derived = match method.as_str() {
            "plain" => verifier.clone(),
            "S256" => {
                use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
                use sha2::{Digest, Sha256};
                let digest = Sha256::digest(verifier.as_bytes());
                URL_SAFE_NO_PAD.encode(digest)
            }
            other => {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    &json!({
                        "error": "invalid_request",
                        "error_description": format!("unsupported code_challenge_method: {}", other),
                    }),
                ));
            }
        };
        if &derived != challenge {
            return Ok(json_response(
                StatusCode::BAD_REQUEST,
                &json!({
                    "error": "invalid_grant",
                    "error_description": "PKCE verifier did not match the stored challenge",
                }),
            ));
        }
    }

    // Build & sign ID Token.
    let claims = IdTokenClaims {
        issuer: &state.config.issuer,
        sub: &state.config.default_user_sub,
        audience: &state.config.client_id,
        nonce: &pending.nonce,
        email: &state.config.default_user_email,
        name: &state.config.default_user_name,
        lifetime_sec: 600,
    };
    let id_token = sign_id_token(&state.keys, &claims)?;

    // Access token: opaque random; not actually consumed by anything in
    // noye, but emitted because real OIDC IdPs always do.
    let access_token = Uuid::new_v4().to_string();

    let body = json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": 600,
        "id_token": id_token,
        "scope": "openid email profile",
    });
    Ok(json_response(StatusCode::OK, &body))
}

// ── Helpers ──

fn json_response(status: StatusCode, body: &serde_json::Value) -> Response<BoxBody> {
    let bytes = serde_json::to_vec(body).expect("JSON serialization should never fail");
    Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .header("Cache-Control", "no-store")
        .body(Full::new(Bytes::from(bytes)))
        .expect("response build")
}

fn parse_query(s: &str) -> std::collections::HashMap<String, String> {
    s.split('&')
        .filter(|p| !p.is_empty())
        .filter_map(|pair| {
            let mut split = pair.splitn(2, '=');
            let key = split.next()?;
            let value = split.next().unwrap_or("");
            Some((
                urlencoding::decode(key).ok()?.into_owned(),
                urlencoding::decode(value).ok()?.into_owned(),
            ))
        })
        .collect()
}

/// If the client supplied a redirect_uri and state we can echo, surface
/// the error there per OAuth 2.0 §4.1.2.1. Otherwise fall through to a
/// 400 JSON response.
fn error_redirect_or_400(
    redirect_uri: Option<&String>,
    state: Option<&String>,
    error: &str,
    description: &str,
) -> Response<BoxBody> {
    if let Some(ru) = redirect_uri {
        let mut location = format!(
            "{}{}error={}&error_description={}",
            ru,
            if ru.contains('?') { "&" } else { "?" },
            urlencoding::encode(error),
            urlencoding::encode(description),
        );
        if let Some(s) = state {
            location.push_str(&format!("&state={}", urlencoding::encode(s)));
        }
        return Response::builder()
            .status(StatusCode::FOUND)
            .header("Location", location)
            .body(Full::new(Bytes::new()))
            .expect("response build");
    }
    json_response(
        StatusCode::BAD_REQUEST,
        &json!({"error": error, "error_description": description}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_query_handles_basic() {
        let m = parse_query("a=1&b=2");
        assert_eq!(m.get("a"), Some(&"1".to_string()));
        assert_eq!(m.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn parse_query_handles_url_encoding() {
        let m = parse_query("redirect_uri=http%3A%2F%2Flocalhost%3A8787%2Fauth%2Fcallback");
        assert_eq!(
            m.get("redirect_uri"),
            Some(&"http://localhost:8787/auth/callback".to_string())
        );
    }

    #[test]
    fn parse_query_handles_empty_value() {
        let m = parse_query("a=&b=2");
        assert_eq!(m.get("a"), Some(&"".to_string()));
        assert_eq!(m.get("b"), Some(&"2".to_string()));
    }

    #[test]
    fn parse_query_handles_empty_input() {
        let m = parse_query("");
        assert!(m.is_empty());
    }
}
