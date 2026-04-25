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
