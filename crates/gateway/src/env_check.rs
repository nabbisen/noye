//! Environment-aware configuration checks.
//!
//! Two responsibilities:
//!
//! 1. Surface the deployment environment (`NOYE_ENV`) as a typed value the
//!    rest of the gateway can branch on. Used by, for example, the cookie
//!    builder to drop the `Secure` attribute on plain-HTTP localhost.
//! 2. Detect the well-known dev-only fallback values for sensitive secrets,
//!    **in every environment, unconditionally**. The intent is to fail
//!    loudly if a `wrangler deploy` — or a `wrangler dev` reading a stale
//!    `.dev.vars` — still resolves a secret to its convenience default
//!    instead of a value registered with `wrangler secret put` or set
//!    explicitly in `.dev.vars`.
//!
//! These two responsibilities are independent on purpose. It used to be
//! that responsibility 2 only ran when `NOYE_ENV` was *not* `"development"`
//! — which meant the shipped `wrangler.toml`'s own `NOYE_ENV =
//! "development"` disabled the control it existed to enforce (gap G-21).
//! `NOYE_ENV` still governs cookie strictness; it no longer governs
//! whether a leaked secret is caught.
//!
//! ## Why `NOYE_ENV` defaults to production
//!
//! Cloudflare Workers exposes no first-class "is this dev or prod" signal.
//! We require operators to opt into development mode via the env var, and
//! we treat unset (or any unrecognized value) as production. This means a
//! production deploy that forgot the env var still gets the strict
//! defaults — a fail-safe choice. Local development sets `NOYE_ENV =
//! "development"` in its own `.dev.vars` (git-ignored); the committed
//! `wrangler.toml.example` ships `"production"`.

use worker::Env;

#[cfg(test)]
mod tests;

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

/// Well-known dev-fallback values. They no longer ship in
/// `crates/gateway/wrangler.toml` — that file is not committed at all
/// (see `.gitignore`); `crates/gateway/wrangler.toml.example` carries no
/// value for either variable, only instructions to set them via
/// `.dev.vars` locally or `wrangler secret put` in production (Subject
/// 03 / G-21). These are the two exact strings this project has, at any
/// point, shipped as a convenience default, and they are never removed
/// from this list even after the file that once carried them changes —
/// a value published once stays published, and someone's `.dev.vars`
/// from before this fix may still hold it.
///
/// These are intentionally const arrays of `(env_var_name, dev_fallback_value)`
/// tuples so the check has a single source of truth for what counts as
/// leaked; changing one without a reason will surface in unit tests.
pub const KNOWN_DEV_FALLBACKS: &[(&str, &str)] = &[
    ("OIDC_CLIENT_SECRET", "dev-idp-does-not-verify-this"),
    ("GATEWAY_SHARED_TOKEN", "noye-local-dev-shared-token"),
];

/// Pure decision: given what each well-known dev-fallback variable is
/// currently observed as (or `None` if unset), does any of them still
/// hold its dev value? Split out from [`check_no_leaked_dev_fallbacks`]
/// so this — the actual security decision — is host-testable without a
/// `Env` binding (NFR-QA-01).
///
/// Applies **unconditionally**, in every environment, not only in
/// production. There is no development bypass: a value published once
/// stays published, regardless of what `NOYE_ENV` says today. (Until
/// 2026-07-28 this had a `NOYE_ENV == "development"` early return, which
/// meant the shipped `wrangler.toml`'s own `NOYE_ENV = "development"`
/// disabled the exact control it existed to enforce — gap G-21.)
fn find_leaked_fallback(observed: &[(&str, Option<String>)]) -> Result<(), String> {
    for (name, dev_value) in KNOWN_DEV_FALLBACKS {
        let is_leaked = observed
            .iter()
            .any(|(n, v)| n == name && v.as_deref() == Some(*dev_value));
        if is_leaked {
            // Tell the operator exactly which variable, but never log
            // the value itself even though it is publicly known: the
            // log line might be tailed unsafely.
            return Err(format!(
                "configuration error: {} has its development-fallback value. \
                     Register the real value with `wrangler secret put {}`, or for local \
                     development set a different value in .dev.vars.",
                name, name
            ));
        }
    }
    Ok(())
}

/// Look up each well-known dev-fallback variable and reject the request
/// (return `Err`) if any of them still hold the dev value — in every
/// environment. This catches the most common deploy mistake: the
/// operator never registered the real Wrangler secret, so `env.var()`
/// still resolves to the convenience default.
pub fn check_no_leaked_dev_fallbacks(env: &Env) -> Result<(), String> {
    let observed: Vec<(&str, Option<String>)> = KNOWN_DEV_FALLBACKS
        .iter()
        // We read via env.var() (not env.secret()) deliberately: the dev
        // fallback, when present, exists as a [vars] entry (or a
        // .dev.vars line, which wrangler dev also exposes via env.var()).
        // Production is supposed to shadow it with a Wrangler secret, but
        // the gateway's resolution order reads secret first and falls
        // back to var. If the operator forgot to register the secret,
        // env.var() returns the dev string.
        .map(|(name, _)| (*name, env.var(name).ok().map(|v| v.to_string())))
        .collect();
    find_leaked_fallback(&observed)
}
