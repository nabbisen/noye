//! Environment-aware checks for the Core worker.
//!
//! The Core has only one well-known dev-fallback variable
//! (`GATEWAY_SHARED_TOKEN`) and no cookie or session concerns of its own,
//! so this module is a deliberately minimal mirror of the gateway's
//! `env_check`.
//!
//! Both workers must agree on what counts as "development" so that the
//! single shared-secret value can flow through the gateway's
//! [`KNOWN_DEV_FALLBACKS`](../../gateway/src/env_check.rs) check.

use worker::Env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Environment {
    Development,
    Production,
}

impl Environment {
    pub fn from_env(env: &Env) -> Self {
        match env.var("NOYE_ENV") {
            Ok(v) => Self::parse(&v.to_string()),
            Err(_) => Self::Production,
        }
    }

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

/// Well-known dev-fallback values that ship in `crates/core/wrangler.toml`'s
/// `[vars]` block for `wrangler dev` convenience.
pub const KNOWN_DEV_FALLBACKS: &[(&str, &str)] = &[
    ("GATEWAY_SHARED_TOKEN", "noye-local-dev-shared-token"),
];

/// In production, reject the request if any well-known dev-fallback value
/// is still in place. This is the Core mirror of the gateway's check; both
/// must run because each worker reads its own configuration.
pub fn check_no_leaked_dev_fallbacks(env: &Env) -> Result<(), String> {
    if Environment::from_env(env).is_development() {
        return Ok(());
    }
    for (name, dev_value) in KNOWN_DEV_FALLBACKS {
        if let Ok(observed) = env.var(name) {
            if observed.to_string() == *dev_value {
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
    fn parse_development() {
        assert_eq!(Environment::parse("development"), Environment::Development);
        assert_eq!(Environment::parse("Development"), Environment::Development);
    }

    #[test]
    fn parse_defaults_to_production() {
        assert_eq!(Environment::parse(""), Environment::Production);
        assert_eq!(Environment::parse("dev"), Environment::Production);
        assert_eq!(Environment::parse("staging"), Environment::Production);
    }

    #[test]
    fn known_dev_fallback_has_gateway_shared_token() {
        let names: Vec<&str> = KNOWN_DEV_FALLBACKS.iter().map(|(n, _)| *n).collect();
        assert_eq!(names, vec!["GATEWAY_SHARED_TOKEN"]);
    }

    #[test]
    fn known_dev_fallback_value_matches_wrangler_toml() {
        let token = KNOWN_DEV_FALLBACKS
            .iter()
            .find(|(n, _)| *n == "GATEWAY_SHARED_TOKEN")
            .map(|(_, v)| *v)
            .unwrap();
        assert_eq!(token, "noye-local-dev-shared-token");
    }
}
