use noye_shared::CreateMaintenanceInput;
use worker::*;

use crate::{api, db};

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let d = ctx.env.d1("DB")?;
    let list = db::maintenance::list_active(&d).await?;
    Response::from_json(&list)
}

pub async fn create(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let body: CreateMaintenanceInput = req.json().await?;
    let mw = db::maintenance::create(&d, &body, &caller).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "maintenance",
        &mw.id,
        "create",
        None,
        Some(&serde_json::to_string(&mw).unwrap_or_default()),
    )
    .await;

    api::with_audit_outcome(Response::from_json(&mw)?, recorded)
}
