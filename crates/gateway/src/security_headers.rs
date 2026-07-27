//! HTTP security headers applied to every response.
//!
//! Centralizing this keeps the policy easy to audit. The full list is set
//! by [`apply`], which is called by every response helper in `lib.rs` —
//! `html_response`, `redirect`, `error_response`, and the streaming
//! responses for CSV / JSON exports.
//!
//! ## Why these specific values
//!
//! - **Content-Security-Policy** — `default-src 'self'` blocks all
//!   third-party loads; `script-src 'self' 'unsafe-inline'` permits the
//!   inline scripts the UI uses today. Long-term, those scripts can be
//!   migrated to a nonce-based CSP, but the current shape is the
//!   tightest CSP that actually permits the existing UI to function.
//! - **frame-ancestors 'none'** plus the legacy `X-Frame-Options: DENY`
//!   blocks all clickjacking; both are sent because some older proxies
//!   only respect the legacy header.
//! - **Strict-Transport-Security** is set to one year with
//!   `includeSubDomains`. We deliberately omit `preload` so operators can
//!   roll back if a subdomain is added that does not yet support HTTPS.
//! - **Referrer-Policy** `no-referrer` keeps internal URLs (which often
//!   embed target IDs) from leaking to upstream notification endpoints
//!   when an admin clicks an external link from a Noye page.
//! - **X-Content-Type-Options** disables MIME-sniffing; CSV responses
//!   must keep their declared `text/csv` content type even on browsers
//!   that try to "help".
//! - **Permissions-Policy** disables every feature Noye does not need
//!   (camera, microphone, geolocation, etc.) so a future XSS cannot use
//!   them.

use worker::{Headers, Result};

/// Apply Noye's security headers to a Headers object in place.
///
/// Idempotent — calling this twice on the same Headers produces the same
/// final state. Existing values are overwritten on conflict.
pub fn apply(headers: &Headers) -> Result<()> {
    headers.set(
        "Content-Security-Policy",
        "default-src 'self'; \
         script-src 'self' 'unsafe-inline'; \
         style-src 'self' 'unsafe-inline'; \
         img-src 'self' data:; \
         connect-src 'self'; \
         frame-ancestors 'none'; \
         form-action 'self'; \
         base-uri 'self'; \
         object-src 'none'",
    )?;
    headers.set(
        "Strict-Transport-Security",
        "max-age=31536000; includeSubDomains",
    )?;
    headers.set("X-Frame-Options", "DENY")?;
    headers.set("X-Content-Type-Options", "nosniff")?;
    headers.set("Referrer-Policy", "no-referrer")?;
    headers.set(
        "Permissions-Policy",
        "accelerometer=(), \
         camera=(), \
         geolocation=(), \
         gyroscope=(), \
         magnetometer=(), \
         microphone=(), \
         payment=(), \
         usb=()",
    )?;
    Ok(())
}

/// Pure-logic test helper: return the policy values that `apply` would
/// install, as a Vec of (header, value) pairs. Used by unit tests so we
/// can verify the policy without a `worker::Headers` object.
#[cfg(test)]
pub fn policy_pairs() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Content-Security-Policy", "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'; form-action 'self'; base-uri 'self'; object-src 'none'"),
        ("Strict-Transport-Security", "max-age=31536000; includeSubDomains"),
        ("X-Frame-Options", "DENY"),
        ("X-Content-Type-Options", "nosniff"),
        ("Referrer-Policy", "no-referrer"),
        ("Permissions-Policy", "accelerometer=(), camera=(), geolocation=(), gyroscope=(), magnetometer=(), microphone=(), payment=(), usb=()"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_includes_all_required_headers() {
        let pairs = policy_pairs();
        let names: Vec<&str> = pairs.iter().map(|(n, _)| *n).collect();
        for required in [
            "Content-Security-Policy",
            "Strict-Transport-Security",
            "X-Frame-Options",
            "X-Content-Type-Options",
            "Referrer-Policy",
            "Permissions-Policy",
        ] {
            assert!(names.contains(&required), "missing {}", required);
        }
    }

    #[test]
    fn csp_blocks_clickjacking_and_inline_object_tags() {
        let pairs = policy_pairs();
        let csp = pairs
            .iter()
            .find(|(n, _)| *n == "Content-Security-Policy")
            .map(|(_, v)| *v)
            .unwrap();
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("base-uri 'self'"));
    }

    #[test]
    fn csp_default_src_is_self() {
        let csp = policy_pairs()
            .into_iter()
            .find(|(n, _)| *n == "Content-Security-Policy")
            .map(|(_, v)| v)
            .unwrap();
        assert!(csp.contains("default-src 'self'"));
    }

    #[test]
    fn x_frame_options_is_deny() {
        let pairs = policy_pairs();
        let xfo = pairs.iter().find(|(n, _)| *n == "X-Frame-Options").map(|(_, v)| *v).unwrap();
        assert_eq!(xfo, "DENY");
    }

    #[test]
    fn x_content_type_options_is_nosniff() {
        let pairs = policy_pairs();
        let xcto = pairs.iter().find(|(n, _)| *n == "X-Content-Type-Options").map(|(_, v)| *v).unwrap();
        assert_eq!(xcto, "nosniff");
    }

    #[test]
    fn hsts_is_at_least_one_year() {
        let pairs = policy_pairs();
        let hsts = pairs.iter().find(|(n, _)| *n == "Strict-Transport-Security").map(|(_, v)| *v).unwrap();
        assert!(hsts.contains("max-age=31536000"));
        assert!(hsts.contains("includeSubDomains"));
    }

    #[test]
    fn referrer_policy_is_strict() {
        let pairs = policy_pairs();
        let rp = pairs.iter().find(|(n, _)| *n == "Referrer-Policy").map(|(_, v)| *v).unwrap();
        assert_eq!(rp, "no-referrer");
    }

    #[test]
    fn permissions_policy_disables_invasive_features() {
        let pairs = policy_pairs();
        let pp = pairs.iter().find(|(n, _)| *n == "Permissions-Policy").map(|(_, v)| *v).unwrap();
        for feature in ["camera=()", "microphone=()", "geolocation=()", "payment=()"] {
            assert!(pp.contains(feature), "permissions policy should deny {}", feature);
        }
    }
}
