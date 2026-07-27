use worker::*;

use crate::{api, db};

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let limit = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(200);
    let logs = db::audit::list_recent(&d, limit).await?;
    Response::from_json(&logs)
}

/// Run a hash-chain integrity check over the entire `audit_logs` table.
///
/// Admin-only. The response is a `ChainVerification` JSON document with
/// counts of legacy / verified / tampered rows; the `tampered_rows` field
/// lists each problem row with a human-readable reason. An empty
/// `tampered_rows` array means the chain is intact (or the only rows are
/// pre-hash-chain legacy ones).
pub async fn verify(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let report = db::audit::verify_chain(&d).await?;
    Response::from_json(&report)
}

/// List the calling user's own recent login events.
///
/// Available to any authenticated user (not admin-only). The query is
/// scoped to the caller's `user_id`, so a user only sees their own
/// history. Admins use `list` (the unfiltered endpoint) to see everyone's.
pub async fn login_history(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;

    let d = ctx.env.d1("DB")?;
    let limit = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(20);
    let logs = db::audit::list_login_history(&d, &caller.user_id, limit).await?;
    Response::from_json(&logs)
}

/// Record a successful login event into `audit_logs`.
///
/// Called by the Gateway from the OIDC callback after a fresh session is
/// minted. Distinct from the Caller-header authenticated endpoints because
/// the request body identifies the *just-logged-in* user (no caller header
/// could exist yet — the session that would back it has not been read by
/// the user's browser).
///
/// Trust comes from the Service Binding shared-token check that already
/// gates every Core call; without it, this endpoint would be
/// impersonation-prone.
pub async fn record_login(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Service Binding gate — same shared-token check as every other
    // mutating Core endpoint. The X-Caller-* check would fail here because
    // no session exists yet.
    api::verify_gateway_token_env(&req, &ctx.env)?;

    #[derive(serde::Deserialize)]
    struct Body {
        user_id: String,
        user_email: String,
        ip_address: Option<String>,
    }
    let body: Body = req.json().await?;

    let d = ctx.env.d1("DB")?;
    db::audit::log_login(
        &d,
        &body.user_id,
        &body.user_email,
        body.ip_address.as_deref(),
    )
    .await?;
    Response::ok("recorded")
}
