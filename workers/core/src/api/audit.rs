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
