use noye_shared::ResolveIncidentInput;
use worker::*;

use crate::{api, db};

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let d = ctx.env.d1("DB")?;
    let limit = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(100);
    let list = db::incidents::list_recent(&d, limit).await?;
    Response::from_json(&list)
}

pub async fn resolve(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let id = ctx.param("id").unwrap().to_string();
    let d = ctx.env.d1("DB")?;
    let body: ResolveIncidentInput = req.json().await?;
    db::incidents::resolve(&d, &id, body.note.as_deref(), &caller).await?;

    let _ = db::audit::log(&d, &caller, "incident", &id, "manual_resolve", None, None).await;

    Response::ok("resolved")
}

/// Target-scoped, window-scoped incident list. Used by the per-target SLA
/// detail page to show "what actually happened during the period the SLA
/// number reflects." The window is parsed with `stats::parse_window`, so the
/// supported formats stay in lockstep with the rest of the SLA surface.
pub async fn list_for_target_in_window(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;

    let url = req.url()?;
    let raw_window = url
        .query_pairs()
        .find(|(k, _)| k == "window")
        .map(|(_, v)| v.to_string());
    let window_sec = match raw_window {
        None => 86_400_i64, // default 24h
        Some(ref s) => crate::stats::parse_window(s)
            .ok_or_else(|| Error::RustError(format!("invalid window: {}", s)))?,
    };

    let now = chrono::Utc::now();
    let window_start = now - chrono::Duration::seconds(window_sec);
    let ws = window_start.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let we = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();

    let list = db::incidents::list_in_window(&d, id, &ws, &we).await?;
    Response::from_json(&list)
}
