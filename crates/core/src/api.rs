//! Internal API layer for the Core worker.
//!
//! Only Service Binding invocations (HTTP) from the Gateway are accepted.
//! Because this worker is not exposed as a route (`workers_dev = false` in wrangler.toml),
//! it is unreachable from the public Internet. As defense in depth, the `X-Gateway-Token` header is also validated.

pub mod audit;
pub mod channels;
pub mod incidents;
pub mod maintenance;
pub mod migration;
pub mod stats;
pub mod targets;
pub mod users;

use noye_shared::{header, Caller};
use worker::*;

/// Admin-only guard.
pub fn require_admin(caller: &Caller) -> Result<()> {
    if caller.is_admin() {
        Ok(())
    } else {
        Err(Error::RustError("FORBIDDEN: admin required".to_string()))
    }
}

/// Verify the Gateway shared secret.
///
/// The Core has no public route and is already isolated from the Internet, but
/// as a guard against configuration mistakes (a stray route, a future Service
/// Binding hijacking via misconfigured `wrangler.toml`, etc.) we keep this
/// defense in depth.
///
/// **Fail-closed**: if `GATEWAY_SHARED_TOKEN` is not registered as a Wrangler
/// secret AND not present as an env var, every request is rejected. Local
/// development gets a `[vars]` fallback in `crates/core/wrangler.toml` so
/// `wrangler dev` works out of the box; production must register the actual
/// secret with `wrangler secret put GATEWAY_SHARED_TOKEN`.
pub fn verify_gateway_token_env(req: &Request, env: &Env) -> Result<()> {
    let expected = match env.secret("GATEWAY_SHARED_TOKEN") {
        Ok(v) => v.to_string(),
        Err(_) => match env.var("GATEWAY_SHARED_TOKEN") {
            Ok(v) => v.to_string(),
            Err(_) => {
                // Fail closed. A previous version of this function returned
                // Ok(()) here for local-dev convenience, but that meant a
                // production deploy that forgot `wrangler secret put`
                // accepted unauthenticated `X-Caller-*` headers from
                // anything that could reach the Service Binding.
                return Err(Error::RustError(
                    "FORBIDDEN: GATEWAY_SHARED_TOKEN not configured (set as Wrangler secret or [vars] fallback)".to_string(),
                ));
            }
        },
    };
    if expected.is_empty() {
        // Treat empty string the same as "missing" — otherwise an env
        // variable accidentally set to "" would re-introduce the bypass.
        return Err(Error::RustError(
            "FORBIDDEN: GATEWAY_SHARED_TOKEN is empty".to_string(),
        ));
    }
    let got = req.headers().get(header::GATEWAY_TOKEN)?;
    if got.as_deref() != Some(expected.as_str()) {
        return Err(Error::RustError(
            "FORBIDDEN: invalid gateway token".to_string(),
        ));
    }
    Ok(())
}

/// Extract caller information that the Gateway injected into the request.
pub fn require_caller_with_env(req: &Request, env: &Env) -> Result<Caller> {
    verify_gateway_token_env(req, env)?;

    let h = req.headers();
    let user_id = h
        .get(header::CALLER_USER_ID)?
        .ok_or_else(|| Error::RustError("Missing X-Caller-UserId".to_string()))?;
    let email = h
        .get(header::CALLER_EMAIL)?
        .ok_or_else(|| Error::RustError("Missing X-Caller-Email".to_string()))?;
    let name = h.get(header::CALLER_NAME)?.unwrap_or_else(|| email.clone());
    let role = h
        .get(header::CALLER_ROLE)?
        .ok_or_else(|| Error::RustError("Missing X-Caller-Role".to_string()))?;

    Ok(Caller { user_id, email, name, role })
}
