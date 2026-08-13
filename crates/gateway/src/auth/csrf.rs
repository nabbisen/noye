//! CSRF token generation and constant-time comparison.
//!
//! Noye uses the Synchronizer Token Pattern: when a session is created, a
//! random 32-byte token is generated, base64url-encoded, stashed alongside
//! the session in KV, and surfaced to the browser via a `<meta name="csrf-
//! token">` tag in every authenticated HTML page. The gateway compares a
//! presented token (in constant time) to the one stored on the session.
//! **The token travels two ways, depending on the route:**
//!
//! - Every JSON API route (`fetch()`-driven): the browser-side code
//!   copies the meta tag's value into an `X-CSRF-Token` header.
//! - `POST /maintenance` (subject 11, NFR-A11Y-10): a native
//!   `<form method="post">` cannot set a custom header, so the token
//!   travels as a hidden `csrf_token` form field instead. Both paths
//!   funnel into the same comparison (`lib::verify_csrf_token`) —
//!   `lib::verify_csrf` extracts from the header, `lib::verify_csrf_form`
//!   extracts from the form field. Neither transport is laxer than the
//!   other: empty, malformed, no-session and mismatch are all rejected
//!   identically regardless of where the token came from.
//!
//! **The form-field transport's safety rests on the session cookie's
//! `SameSite=Lax` attribute (`auth::cookie`), not on anything in this
//! module.** A native cross-origin `<form method="post">` is a "simple
//! request" — unlike a cross-origin `fetch()` carrying a custom header,
//! it triggers no CORS preflight — so nothing here stops a third-party
//! page from submitting one. What stops it is that `SameSite=Lax` keeps
//! the session cookie off cross-site POSTs, so the request arrives with
//! no session to check the presented token against, and hits the
//! "no active session" rejection before comparison. If that cookie
//! attribute is ever relaxed, this is the route it costs.
//!
//! ## Why constant-time comparison
//!
//! A naive `==` over byte slices in Rust short-circuits on the first
//! mismatching byte, which leaks position-by-position information about
//! the correct token via timing observation. Constant-time comparison
//! removes that channel. The 32-byte random token is large enough that
//! brute force is infeasible without timing leaks anyway, but the leak-
//! free baseline costs nothing extra and is the textbook practice.
//!
//! ## What this module does NOT do
//!
//! - It does not interact with KV — that's `auth::session`.
//! - It does not extract the token from a Request or FormData — that's
//!   `lib::verify_csrf` / `lib::verify_csrf_form`.
//! - It does not encode/decode base64 — it just shapes random bytes into
//!   a printable form via `crypto::random::random_token`.

use crate::auth::crypto;

/// Number of random bytes in a fresh CSRF token. 32 bytes = 256 bits, which
/// matches the session token's strength and is the standard recommendation
/// for OWASP-class anti-CSRF tokens.
pub const TOKEN_BYTES: usize = 32;

/// Generate a fresh CSRF token. Returns a base64url-encoded string of
/// length 43 (32 bytes → 43 chars without padding).
pub fn generate() -> worker::Result<String> {
    let bytes = crypto::random_bytes(TOKEN_BYTES)
        .map_err(|e| worker::Error::RustError(format!("csrf rng failed: {}", e)))?;
    Ok(crypto::base64url_encode(&bytes))
}

/// Compare two tokens in constant time over their byte representations.
///
/// Both strings should be base64url-encoded outputs from [`generate`]. We
/// length-check first (different-length inputs cannot match), then walk
/// the shorter slice and OR up the byte differences without short-
/// circuiting.
///
/// Pure helper — does not touch any I/O — so unit tests can pin the timing
/// invariants without a worker runtime.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    // Single-byte XOR accumulator: any mismatching position flips a bit
    // that survives all subsequent ORs. Final equality check returns true
    // iff every byte position matched.
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Sanity check on the shape of a CSRF token read from a request header.
///
/// Tokens we generate are exactly 43 base64url characters (no padding).
/// Anything else is an obviously-bad value we can reject before doing the
/// constant-time comparison — but we still do the comparison after, so
/// the rejection path doesn't leak via timing.
pub fn looks_well_formed(s: &str) -> bool {
    s.len() == 43
        && s.bytes()
            .all(|b| matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constant_time_eq_matches_identical_strings() {
        assert!(constant_time_eq("abc", "abc"));
        assert!(constant_time_eq("", ""));
        assert!(constant_time_eq(
            "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde",
            "AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde",
        ));
    }

    #[test]
    fn constant_time_eq_rejects_distinct_strings() {
        assert!(!constant_time_eq("abc", "abd"));
        assert!(!constant_time_eq("abc", "xyz"));
    }

    #[test]
    fn constant_time_eq_rejects_different_lengths() {
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("abcd", "abc"));
        assert!(!constant_time_eq("", "a"));
    }

    #[test]
    fn constant_time_eq_rejects_first_byte_mismatch() {
        // Sanity check that a difference at the start is caught (regression
        // guard against accidentally short-circuiting in the loop).
        assert!(!constant_time_eq("Xbcdefg", "Abcdefg"));
    }

    #[test]
    fn constant_time_eq_rejects_last_byte_mismatch() {
        // And at the end — the OR-accumulator must keep all positions.
        assert!(!constant_time_eq("abcdefX", "abcdefA"));
    }

    #[test]
    fn looks_well_formed_accepts_43_char_base64url() {
        let s: String = std::iter::repeat_n('A', 43).collect();
        assert!(looks_well_formed(&s));
    }

    #[test]
    fn looks_well_formed_accepts_full_base64url_alphabet() {
        let s = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmno_-";
        assert_eq!(s.len(), 43);
        assert!(looks_well_formed(s));
    }

    #[test]
    fn looks_well_formed_rejects_wrong_length() {
        assert!(!looks_well_formed(""));
        assert!(!looks_well_formed("abc"));
        let too_long: String = std::iter::repeat_n('A', 44).collect();
        assert!(!looks_well_formed(&too_long));
        let too_short: String = std::iter::repeat_n('A', 42).collect();
        assert!(!looks_well_formed(&too_short));
    }

    #[test]
    fn looks_well_formed_rejects_padding_chars() {
        // Standard base64 (with `=` padding and `/`, `+` characters) should
        // not pass — we explicitly chose URL-safe encoding.
        let s_eq = format!("{}{}", "A".repeat(42), "=");
        let s_slash = format!("{}{}", "A".repeat(42), "/");
        let s_plus = format!("{}{}", "A".repeat(42), "+");
        assert!(!looks_well_formed(&s_eq));
        assert!(!looks_well_formed(&s_slash));
        assert!(!looks_well_formed(&s_plus));
    }

    #[test]
    fn looks_well_formed_rejects_whitespace_and_control() {
        let s_space = format!("{} {}", "A".repeat(21), "A".repeat(21));
        assert_eq!(s_space.len(), 43);
        assert!(!looks_well_formed(&s_space));
    }
}
