use worker::*;

/// Read the value of a specific cookie from the request headers.
///
/// Targets the `Cookie` header (client-to-server), not `Set-Cookie`.
/// Delegates parsing to [`parse_cookie_header`] so the parsing logic can be
/// unit-tested without a `worker::Request`.
pub fn get(req: &Request, name: &str) -> Result<Option<String>> {
    let cookie_header = match req.headers().get("Cookie")? {
        Some(v) => v,
        None => return Ok(None),
    };
    Ok(parse_cookie_header(&cookie_header, name))
}

/// Build a Set-Cookie header value.
///
/// Defaults to the safest settings (HttpOnly + Secure + SameSite=Lax + Path=/).
/// We use SameSite=Lax (not Strict) so the cookie is sent on the OIDC callback navigation.
pub struct CookieBuilder {
    name: String,
    value: String,
    max_age_sec: Option<i64>,
    path: String,
    same_site: String,
    http_only: bool,
    secure: bool,
}

impl CookieBuilder {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
            max_age_sec: None,
            path: "/".to_string(),
            same_site: "Lax".to_string(),
            http_only: true,
            secure: true,
        }
    }

    pub fn max_age(mut self, seconds: i64) -> Self {
        self.max_age_sec = Some(seconds);
        self
    }

    /// Override the `Secure` attribute. Default is `true`. Set to `false` only
    /// for local development on plain-HTTP origins (`http://localhost`) where
    /// `Secure` cookies are not delivered by some browsers/proxies.
    ///
    /// Production deploys should never call this with `false` — the gateway
    /// is HTTPS-only via Cloudflare's edge and the `Secure` attribute is the
    /// last line of defense against accidental plaintext transmission.
    pub fn secure(mut self, secure: bool) -> Self {
        self.secure = secure;
        self
    }

    /// For logout: immediately-expired cookie (Max-Age=0).
    ///
    /// `Secure` defaults to true; chain `.secure(false)` for local-dev clearing.
    pub fn expired(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: String::new(),
            max_age_sec: Some(0),
            path: "/".to_string(),
            same_site: "Lax".to_string(),
            http_only: true,
            secure: true,
        }
    }

    pub fn build(self) -> String {
        let mut parts = vec![format!("{}={}", self.name, self.value)];
        parts.push(format!("Path={}", self.path));
        parts.push(format!("SameSite={}", self.same_site));
        if self.http_only {
            parts.push("HttpOnly".to_string());
        }
        if self.secure {
            parts.push("Secure".to_string());
        }
        if let Some(sec) = self.max_age_sec {
            parts.push(format!("Max-Age={}", sec));
        }
        parts.join("; ")
    }
}

/// Pure helper that parses a `Cookie:` request-header value and returns the
/// value of the named cookie, if present.
///
/// Split out from [`get`] so the parsing logic can be unit-tested without a
/// `worker::Request`.
pub fn parse_cookie_header(header: &str, name: &str) -> Option<String> {
    for pair in header.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=') {
            if k.trim() == name {
                return Some(v.trim().to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Builder tests ──

    #[test]
    fn build_sets_secure_defaults() {
        let cookie = CookieBuilder::new("sid", "abc").build();
        assert!(cookie.starts_with("sid=abc"));
        assert!(cookie.contains("Path=/"));
        assert!(cookie.contains("SameSite=Lax"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn build_max_age_attribute() {
        let cookie = CookieBuilder::new("sid", "abc").max_age(3600).build();
        assert!(cookie.contains("Max-Age=3600"));
    }

    #[test]
    fn build_no_max_age_when_unset() {
        let cookie = CookieBuilder::new("sid", "abc").build();
        assert!(!cookie.contains("Max-Age="));
    }

    #[test]
    fn build_expired_cookie_uses_max_age_zero() {
        let cookie = CookieBuilder::expired("sid").build();
        assert!(cookie.starts_with("sid=;"));
        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("HttpOnly"));
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn build_omits_secure_when_disabled() {
        let cookie = CookieBuilder::new("sid", "abc").secure(false).build();
        assert!(cookie.starts_with("sid=abc"));
        assert!(cookie.contains("HttpOnly"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn build_includes_secure_by_default_chain() {
        // `.secure(true)` is a no-op on the default but should remain
        // explicit-true at the end.
        let cookie = CookieBuilder::new("sid", "abc").secure(true).build();
        assert!(cookie.contains("Secure"));
    }

    #[test]
    fn build_expired_supports_secure_setter() {
        let cookie = CookieBuilder::expired("sid").secure(false).build();
        assert!(cookie.contains("Max-Age=0"));
        assert!(!cookie.contains("Secure"));
    }

    // ── Parser tests ──

    #[test]
    fn parse_returns_named_cookie_value() {
        let header = "sid=xyz; theme=dark";
        assert_eq!(parse_cookie_header(header, "sid"), Some("xyz".into()));
        assert_eq!(parse_cookie_header(header, "theme"), Some("dark".into()));
    }

    #[test]
    fn parse_handles_extra_whitespace_around_pairs() {
        let header = "  sid = xyz ;  theme=dark";
        assert_eq!(parse_cookie_header(header, "sid"), Some("xyz".into()));
    }

    #[test]
    fn parse_returns_none_when_missing() {
        let header = "theme=dark";
        assert_eq!(parse_cookie_header(header, "sid"), None);
    }

    #[test]
    fn parse_returns_none_for_empty_header() {
        assert_eq!(parse_cookie_header("", "sid"), None);
    }

    #[test]
    fn parse_handles_value_containing_equals_sign() {
        // base64-style values can include "=" padding
        let header = "sid=YWJjZA==; theme=dark";
        // split_once on the first "=" preserves the rest of the value
        assert_eq!(parse_cookie_header(header, "sid"), Some("YWJjZA==".into()));
    }

    #[test]
    fn parse_first_match_when_duplicates() {
        // RFC 6265 doesn't define order; we take the first occurrence as we iterate.
        let header = "sid=first; sid=second";
        assert_eq!(parse_cookie_header(header, "sid"), Some("first".into()));
    }
}

