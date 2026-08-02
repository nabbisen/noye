//! Internal API handlers for monitored targets.
//!
//! Receives calls from the Gateway and delegates to the DB module.

use noye_shared::{CreateTargetInput, UpdateTargetInput};
use worker::*;

use crate::{api, db};

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    let d = ctx.env.d1("DB")?;
    let targets = db::targets::list_all(&d, &caller).await?;
    Response::from_json(&targets)
}

pub async fn get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    let target = db::targets::get_by_id(&d, id).await?;
    Response::from_json(&target)
}

pub async fn create(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let body: CreateTargetInput = req.json().await?;
    let target = db::targets::create(&d, &body, &caller).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "target",
        &target.id,
        "create",
        None,
        Some(&serde_json::to_string(&target).unwrap_or_default()),
    )
    .await;

    api::with_audit_outcome(Response::from_json(&target)?, recorded)
}

pub async fn update(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let id = ctx.param("id").unwrap().to_string();
    let d = ctx.env.d1("DB")?;
    let old = db::targets::get_by_id(&d, &id).await?;

    let body: UpdateTargetInput = req.json().await?;
    let updated = db::targets::update(&d, &id, &body, &caller).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "target",
        &id,
        "update",
        Some(&serde_json::to_string(&old).unwrap_or_default()),
        Some(&serde_json::to_string(&updated).unwrap_or_default()),
    )
    .await;

    api::with_audit_outcome(Response::from_json(&updated)?, recorded)
}

pub async fn delete(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    db::targets::delete(&d, id).await?;

    let recorded = db::audit::log_or_report(&d, &caller, "target", id, "delete", None, None).await;

    api::with_audit_outcome(Response::ok("deleted")?, recorded)
}

pub async fn summary(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let d = ctx.env.d1("DB")?;
    let summary = db::targets::get_status_summary(&d).await?;
    Response::from_json(&summary)
}

pub async fn states(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let d = ctx.env.d1("DB")?;
    let states = db::states::list_all(&d).await?;
    Response::from_json(&states)
}

pub async fn state_for(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    let state = db::states::get_by_target(&d, id).await?;
    Response::from_json(&state)
}

pub async fn results(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    let limit = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "limit")
        .and_then(|(_, v)| v.parse::<i64>().ok())
        .unwrap_or(50);
    let results = db::results::list_recent(&d, id, limit).await?;
    Response::from_json(&results)
}
