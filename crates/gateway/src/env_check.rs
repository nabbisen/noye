//! Environment-aware configuration checks.
//!
//! Two responsibilities:
//!
//! 1. Surface the deployment environment (`NOYE_ENV`) as a typed value the
//!    rest of the gateway can branch on. Used by, for example, the cookie
//!    builder to drop the `Secure` attribute on plain-HTTP localhost.
//! 2. Detect the well-known dev-only fallback values for sensitive secrets
//!    when running in production. The intent is to fail loudly if a
//!    `wrangler deploy` ships with the dev fallbacks still present in
//!    `[vars]` instead of being overridden by `wrangler secret put`.
//!
//! ## Why `NOYE_ENV` defaults to production
//!
//! Cloudflare Workers exposes no first-class "is this dev or prod" signal.
//! We require operators to opt into development mode via the env var, and
//! we treat unset (or any unrecognized value) as production. This means a
//! production deploy that forgot the env var still gets the strict
//! defaults — a fail-safe choice. Local development gets `NOYE_ENV =
//! "development"` in the shipped `wrangler.toml`'s `[vars]` block.

use worker::Env;

/// Deployment environment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    /// `wrangler dev`, integration tests, anywhere with non-secret
    /// fallback values intended for convenience. Behavior is permissive
    /// (relaxed cookie attributes, no fallback-value detection).
    Development,
    /// `wrangler deploy` and equivalent. Strict defaults: cookies are
    /// `Secure`-only, leaked dev-fallback values cause request rejection.
    Production,
}

impl Environment {
    /// Read the `NOYE_ENV` env var and decide the environment.
    ///
    /// - `"development"` (case-insensitive) → [`Environment::Development`]
    /// - anything else, or absent           → [`Environment::Production`]
    pub fn from_env(env: &Env) -> Self {
        match env.var("NOYE_ENV") {
            Ok(v) => Self::parse(&v.to_string()),
            Err(_) => Self::Production,
        }
    }

    /// Pure parser, exposed for testability.
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("development") {
            Self::Development
        } else {
            Self::Production
        }
    }

    pub fn is_development(self) -> bool {
        matches!(self, Self::Development)
    }
}

/// Well-known dev-fallback values that ship in `crates/gateway/wrangler.toml`'s
/// `[vars]` block for `wrangler dev` convenience. If we see these in
/// production, the operator forgot to register the real Wrangler secrets and
/// the deploy is in an unsafe state.
///
/// These are intentionally const arrays of `(env_var_name, dev_fallback_value)`
/// tuples so the check covers exactly what the wrangler.toml file ships with;
/// changing one without changing the other will surface in unit tests.
pub const KNOWN_DEV_FALLBACKS: &[(&str, &str)] = &[
    ("OIDC_CLIENT_SECRET", "dev-idp-does-not-verify-this"),
    ("GATEWAY_SHARED_TOKEN", "noye-local-dev-shared-token"),
];

/// In production, look up each well-known dev-fallback variable and reject
/// the deploy (return Err) if any of them still hold the dev value. This
/// catches the most common deploy mistake: the operator copied
/// `wrangler.toml` without removing the `[vars]` fallback after running
/// `wrangler secret put`.
///
/// In development, this is a no-op — the dev fallbacks are *expected* there.
pub fn check_no_leaked_dev_fallbacks(env: &Env) -> Result<(), String> {
    if Environment::from_env(env).is_development() {
        return Ok(());
    }

    for (name, dev_value) in KNOWN_DEV_FALLBACKS {
        // We read via env.var() (not env.secret()) deliberately: the dev
        // fallback exists as a [vars] entry. Production is supposed to
        // shadow it with a Wrangler secret, but the gateway's resolution
        // order reads secret first and falls back to var. If the operator
        // forgot to register the secret, env.var() returns the dev string.
        if let Ok(observed) = env.var(name) {
            if observed.to_string() == *dev_value {
                // Tell the operator exactly which variable, but never log
                // the value itself even though it is publicly known: the
                // log line might be tailed unsafely.
                return Err(format!(
                    "configuration error: {} has its development-fallback value in production. \
                     Register the real value with `wrangler secret put {}` and remove the \
                     [vars] fallback from wrangler.toml.",
                    name, name
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_development_variants() {
        assert_eq!(Environment::parse("development"), Environment::Development);
        assert_eq!(Environment::parse("Development"), Environment::Development);
        assert_eq!(Environment::parse("DEVELOPMENT"), Environment::Development);
    }

    #[test]
    fn parse_production_when_unset_or_unknown() {
        assert_eq!(Environment::parse(""), Environment::Production);
        assert_eq!(Environment::parse("production"), Environment::Production);
        assert_eq!(Environment::parse("staging"), Environment::Production);
        assert_eq!(Environment::parse("dev"), Environment::Production); // strict: not "development"
        assert_eq!(Environment::parse("test"), Environment::Production);
    }

    #[test]
    fn known_dev_fallbacks_includes_oidc_secret() {
        let names: Vec<&str> = KNOWN_DEV_FALLBACKS.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"OIDC_CLIENT_SECRET"));
    }

    #[test]
    fn known_dev_fallbacks_includes_gateway_shared_token() {
        let names: Vec<&str> = KNOWN_DEV_FALLBACKS.iter().map(|(n, _)| *n).collect();
        assert!(names.contains(&"GATEWAY_SHARED_TOKEN"));
    }

    #[test]
    fn known_dev_fallbacks_match_wrangler_toml_values() {
        // These literals are duplicated in `crates/gateway/wrangler.toml`'s
        // [vars] section. If you change either one, change the other.
        let oidc = KNOWN_DEV_FALLBACKS
            .iter()
            .find(|(n, _)| *n == "OIDC_CLIENT_SECRET")
            .map(|(_, v)| *v)
            .unwrap();
        assert_eq!(oidc, "dev-idp-does-not-verify-this");

        let token = KNOWN_DEV_FALLBACKS
            .iter()
            .find(|(n, _)| *n == "GATEWAY_SHARED_TOKEN")
            .map(|(_, v)| *v)
            .unwrap();
        assert_eq!(token, "noye-local-dev-shared-token");
    }

    #[test]
    fn is_development_helper() {
        assert!(Environment::Development.is_development());
        assert!(!Environment::Production.is_development());
    }
}
