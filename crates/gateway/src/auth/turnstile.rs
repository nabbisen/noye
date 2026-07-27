//! Cloudflare Turnstile bot-protection helpers.
//!
//! Turnstile is a supplementary defense for *public* forms (login, contact,
//! signup) per the original specification. Within Noye most data-mutating
//! routes already require an authenticated session, so Turnstile is opt-in:
//! a route handler that wants the protection calls [`verify_token`] before
//! committing any side effects, and the corresponding UI renders the widget
//! via [`widget_html`].
//!
//! Configuration:
//! - `TURNSTILE_SITE_KEY` (env var, public): site key embedded in the widget.
//!   When unset or empty, Turnstile is disabled and `verify_token` returns
//!   `Ok(())` without contacting Cloudflare. This makes the helper safe to
//!   call unconditionally during local development.
//! - `TURNSTILE_SECRET_KEY` (Wrangler secret): used to verify tokens server
//!   side. Required only when `TURNSTILE_SITE_KEY` is set.
//!
//! Until the first public form lands, the public helpers below are unused;
//! the module-level `allow(dead_code)` keeps the scaffolding from generating
//! warnings while we wait for a caller.

#![allow(dead_code)]

use serde::Deserialize;
use worker::*;

/// Cloudflare Turnstile siteverify endpoint.
const SITEVERIFY_URL: &str = "https://challenges.cloudflare.com/turnstile/v0/siteverify";

/// Read the site key from env. An empty string means Turnstile is disabled.
fn site_key(env: &Env) -> String {
    env.var("TURNSTILE_SITE_KEY")
        .map(|v| v.to_string())
        .unwrap_or_default()
}

/// Returns true when Turnstile is configured (i.e. a non-empty site key is set).
pub fn is_enabled(env: &Env) -> bool {
    !site_key(env).is_empty()
}

/// Render the `<div>` element that Cloudflare's `turnstile.js` inflates into
/// the actual challenge widget. The caller is responsible for including the
/// Cloudflare-hosted script tag in the page; see [`script_tag_html`].
///
/// When Turnstile is not configured, returns an empty string so callers can
/// embed the widget unconditionally and let configuration drive presence.
pub fn widget_html(env: &Env) -> String {
    let key = site_key(env);
    if key.is_empty() {
        return String::new();
    }
    format!(
        r#"<div class="cf-turnstile" data-sitekey="{}" data-theme="auto" aria-label="Bot-protection challenge"></div>"#,
        html_escape(&key)
    )
}

/// Returns the `<script>` tag that loads the Turnstile JavaScript runtime.
/// Empty when Turnstile is not configured, matching [`widget_html`].
pub fn script_tag_html(env: &Env) -> String {
    if !is_enabled(env) {
        return String::new();
    }
    r#"<script src="https://challenges.cloudflare.com/turnstile/v0/api.js" async defer></script>"#.to_string()
}

/// Verify a Turnstile token submitted from the browser (`cf-turnstile-response`
/// form field). Returns `Ok(())` on success or when Turnstile is disabled, and
/// `Err` on token rejection or verification failure.
///
/// `remote_ip` is optional but recommended; pass `req.headers().get("CF-Connecting-IP")`
/// when available.
pub async fn verify_token(env: &Env, token: &str, remote_ip: Option<&str>) -> Result<()> {
    if !is_enabled(env) {
        return Ok(());
    }

    let secret = env
        .secret("TURNSTILE_SECRET_KEY")
        .map(|v| v.to_string())
        .map_err(|_| Error::RustError(
            "TURNSTILE_SECRET_KEY is not configured but TURNSTILE_SITE_KEY is set".to_string(),
        ))?;

    if token.is_empty() {
        return Err(Error::RustError("missing-input-response".to_string()));
    }

    let body = build_form_body(&secret, token, remote_ip);

    let headers = Headers::new();
    headers.set("Content-Type", "application/x-www-form-urlencoded")?;

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_headers(headers);
    init.with_body(Some(wasm_bindgen::JsValue::from_str(&body)));

    let request = Request::new_with_init(SITEVERIFY_URL, &init)?;
    let mut response = Fetch::Request(request).send().await?;

    let raw = response.text().await?;
    let parsed: SiteverifyResponse = serde_json::from_str(&raw)
        .map_err(|e| Error::RustError(format!("siteverify response parse error: {} body={}", e, raw)))?;

    if !parsed.success {
        let codes = parsed.error_codes.unwrap_or_default().join(",");
        return Err(Error::RustError(format!(
            "Turnstile verification failed: {}",
            if codes.is_empty() { "unknown".to_string() } else { codes }
        )));
    }

    Ok(())
}

/// URL-encoded form body for the siteverify endpoint. Extracted for unit testing.
fn build_form_body(secret: &str, token: &str, remote_ip: Option<&str>) -> String {
    let mut s = format!(
        "secret={}&response={}",
        urlencoding::encode(secret),
        urlencoding::encode(token)
    );
    if let Some(ip) = remote_ip {
        if !ip.is_empty() {
            s.push_str("&remoteip=");
            s.push_str(&urlencoding::encode(ip));
        }
    }
    s
}

/// Subset of the siteverify response we care about (per
/// <https://developers.cloudflare.com/turnstile/get-started/server-side-validation/>).
#[derive(Debug, Deserialize)]
struct SiteverifyResponse {
    success: bool,
    #[serde(rename = "error-codes", default)]
    error_codes: Option<Vec<String>>,
}

/// Minimal HTML escaper for attribute values. Only needs to handle the
/// site-key, which Cloudflare guarantees is alphanumeric, but we apply it
/// defensively in case configuration is fed from an unexpected source.
fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_body_without_ip() {
        let body = build_form_body("sec-ret", "tok-en", None);
        assert_eq!(body, "secret=sec-ret&response=tok-en");
    }

    #[test]
    fn form_body_with_ip() {
        let body = build_form_body("sec-ret", "tok-en", Some("203.0.113.7"));
        assert_eq!(body, "secret=sec-ret&response=tok-en&remoteip=203.0.113.7");
    }

    #[test]
    fn form_body_treats_empty_ip_as_absent() {
        let body = build_form_body("sec-ret", "tok-en", Some(""));
        assert_eq!(body, "secret=sec-ret&response=tok-en");
    }

    #[test]
    fn form_body_url_encodes_special_characters() {
        // Secret values may contain symbols requiring percent-encoding.
        let body = build_form_body("a&b=c", "tok+en", Some("v6:[::1]"));
        assert!(body.starts_with("secret=a%26b%3Dc&response=tok%2Ben"));
        assert!(body.contains("remoteip=v6%3A%5B%3A%3A1%5D"));
    }

    #[test]
    fn html_escape_handles_attribute_special_chars() {
        assert_eq!(html_escape("a&b<c>"), "a&amp;b&lt;c&gt;");
        assert_eq!(html_escape(r#"x"y'z"#), "x&quot;y&#39;z");
    }

    #[test]
    fn siteverify_response_parses_success_with_no_errors() {
        let json = r#"{"success": true}"#;
        let r: SiteverifyResponse = serde_json::from_str(json).unwrap();
        assert!(r.success);
    }

    #[test]
    fn siteverify_response_parses_failure_with_error_codes() {
        let json = r#"{"success": false, "error-codes": ["invalid-input-response", "timeout-or-duplicate"]}"#;
        let r: SiteverifyResponse = serde_json::from_str(json).unwrap();
        assert!(!r.success);
        let codes = r.error_codes.expect("error-codes should be present");
        assert_eq!(codes, vec!["invalid-input-response", "timeout-or-duplicate"]);
    }

    #[test]
    fn siteverify_response_treats_missing_error_codes_as_none() {
        let json = r#"{"success": false}"#;
        let r: SiteverifyResponse = serde_json::from_str(json).unwrap();
        assert!(!r.success);
        assert!(r.error_codes.is_none());
    }
}
