//! Validate redirect destinations supplied via untrusted query parameters.
//!
//! `?return_to=` shows up on `/auth/login` so that successful sign-in can
//! deliver the user back to whatever page they were trying to reach. Without
//! validation, an attacker can craft `?return_to=https://evil.example`
//! and turn the gateway into a phishing relay: the victim recognizes the
//! gateway's hostname in the link, signs in legitimately, and is then
//! redirected to the attacker's site.
//!
//! ## Policy
//!
//! Only same-origin, path-relative URLs are allowed. The accepted shape is:
//!
//! - Starts with `/`
//! - Does NOT start with `//` (which is a protocol-relative URL)
//! - Does NOT contain backslashes (some browsers normalize `\` to `/` and
//!   are confused by mixed slashes; an absolute URL like `/\evil.com/foo`
//!   would otherwise pass)
//! - Does NOT contain CR or LF (header-injection guard for paranoia, even
//!   though hyper/worker would already reject these)
//!
//! Anything else falls back to `/`.

/// Sanitize an incoming `return_to` value. Returns the input if it satisfies
/// the same-origin policy, or `/` otherwise. The result is always a valid
/// path-relative URL the gateway can safely emit in a `Location:` header.
pub fn sanitize_return_to(raw: &str) -> String {
    if is_safe_return_to(raw) {
        raw.to_string()
    } else {
        "/".to_string()
    }
}

/// Pure boolean form of the policy. Pulled out so tests can verify the
/// rules independently from the fallback behavior.
pub fn is_safe_return_to(raw: &str) -> bool {
    if raw.is_empty() {
        return false;
    }
    // Must start with a single forward slash.
    if !raw.starts_with('/') {
        return false;
    }
    // Reject protocol-relative URLs ("//evil.example") which a browser
    // resolves against the current scheme rather than the current origin.
    if raw.starts_with("//") {
        return false;
    }
    // Backslashes can be folded to forward slashes by some browsers,
    // turning "/\evil.example/path" into "//evil.example/path" effectively.
    if raw.contains('\\') {
        return false;
    }
    // CR/LF have no business inside a URL component and serve as a
    // header-injection canary.
    if raw.contains('\r') || raw.contains('\n') {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_root() {
        assert!(is_safe_return_to("/"));
    }

    #[test]
    fn accepts_simple_paths() {
        assert!(is_safe_return_to("/targets"));
        assert!(is_safe_return_to("/stats/abc-123"));
        assert!(is_safe_return_to("/incidents?window=24h"));
        assert!(is_safe_return_to("/admin/migration"));
    }

    #[test]
    fn accepts_path_with_query_and_fragment() {
        assert!(is_safe_return_to("/stats/x?window=7d#row-1"));
    }

    #[test]
    fn rejects_empty() {
        assert!(!is_safe_return_to(""));
    }

    #[test]
    fn rejects_absolute_urls() {
        assert!(!is_safe_return_to("https://evil.example/phish"));
        assert!(!is_safe_return_to("http://noye.example/path"));
        assert!(!is_safe_return_to("javascript:alert(1)"));
        assert!(!is_safe_return_to("data:text/html,<script>"));
    }

    #[test]
    fn rejects_protocol_relative_urls() {
        assert!(!is_safe_return_to("//evil.example"));
        assert!(!is_safe_return_to("//evil.example/path"));
    }

    #[test]
    fn rejects_path_starting_with_backslash_trick() {
        assert!(!is_safe_return_to("/\\evil.example/path"));
        assert!(!is_safe_return_to("/foo/bar\\baz"));
    }

    #[test]
    fn rejects_paths_with_cr_or_lf() {
        assert!(!is_safe_return_to("/foo\r\nLocation: https://evil"));
        assert!(!is_safe_return_to("/foo\nfoo"));
        assert!(!is_safe_return_to("/foo\rfoo"));
    }

    #[test]
    fn rejects_relative_paths_without_leading_slash() {
        assert!(!is_safe_return_to("targets"));
        assert!(!is_safe_return_to("../etc/passwd"));
        assert!(!is_safe_return_to("./foo"));
    }

    #[test]
    fn sanitize_returns_safe_input_unchanged() {
        assert_eq!(sanitize_return_to("/targets"), "/targets");
        assert_eq!(sanitize_return_to("/"), "/");
    }

    #[test]
    fn sanitize_falls_back_to_root_for_unsafe() {
        assert_eq!(sanitize_return_to("https://evil.example"), "/");
        assert_eq!(sanitize_return_to("//evil"), "/");
        assert_eq!(sanitize_return_to(""), "/");
        assert_eq!(sanitize_return_to("foo"), "/");
    }
}
