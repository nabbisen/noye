//! Dev-fallback-leak check for the Core worker.
//!
//! The Core has only one well-known dev-fallback variable
//! (`GATEWAY_SHARED_TOKEN`) and no cookie or session concerns of its own —
//! unlike the gateway, it has no other use for `NOYE_ENV`, so this module
//! does not read it at all. The dev-fallback check applies
//! **unconditionally**, in every environment; see [`find_leaked_fallback`].
//!
//! Until 2026-07-28 this module carried an `Environment` type and an
//! early return on `is_development()`, mirroring the gateway's shape at
//! the time. That meant the shipped `wrangler.toml`'s own `NOYE_ENV =
//! "development"` disabled the control it existed to enforce (gap G-21).
//! Removed rather than fixed-in-place: nothing else in this crate ever
//! read `Environment` or `NOYE_ENV` (`grep` confirms), so keeping the type
//! around post-fix would have been dead code pretending to be a shared
//! concept with the gateway, which has genuine cookie-strictness reasons
//! to keep its own copy.

use worker::Env;

#[cfg(test)]
mod tests;

/// Well-known dev-fallback values. `crates/core/wrangler.toml` is not
/// committed (see `.gitignore`); `wrangler.toml.example` carries no value
/// for this variable, only instructions to set it via `.dev.vars` locally
/// or `wrangler secret put` in production (Subject 03 / G-21). Never
/// removed from this list even after the file that once carried it
/// changes — a value published once stays published.
pub const KNOWN_DEV_FALLBACKS: &[(&str, &str)] =
    &[("GATEWAY_SHARED_TOKEN", "noye-local-dev-shared-token")];

/// Pure decision: given what `GATEWAY_SHARED_TOKEN` is currently observed
/// as (or `None` if unset), does it still hold its dev value? Split out
/// from [`check_no_leaked_dev_fallbacks`] so this is host-testable
/// without an `Env` binding (NFR-QA-01). Applies unconditionally — see
/// the module doc comment.
fn find_leaked_fallback(observed: &[(&str, Option<String>)]) -> Result<(), String> {
    for (name, dev_value) in KNOWN_DEV_FALLBACKS {
        let is_leaked = observed
            .iter()
            .any(|(n, v)| n == name && v.as_deref() == Some(*dev_value));
        if is_leaked {
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

/// Reject the request if any well-known dev-fallback value is still in
/// place — in every environment. This is the Core mirror of the
/// gateway's check; both must run because each worker reads its own
/// configuration.
pub fn check_no_leaked_dev_fallbacks(env: &Env) -> Result<(), String> {
    let observed: Vec<(&str, Option<String>)> = KNOWN_DEV_FALLBACKS
        .iter()
        .map(|(name, _)| (*name, env.var(name).ok().map(|v| v.to_string())))
        .collect();
    find_leaked_fallback(&observed)
}
