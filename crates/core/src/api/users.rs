use noye_shared::{LookupUserResult, ManageUserInput};
use worker::*;

use crate::{api, db};

/// Endpoint that the Gateway calls after OIDC authentication to resolve the user's role.
/// The `X-Caller-*` headers are not required (this lookup happens before Gateway authentication, so only the gateway token is validated).
pub async fn lookup(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    api::verify_gateway_token_env(&req, &ctx.env)?;

    let email = ctx.param("email").unwrap();
    // URL decode (the Gateway encodes before sending)
    let email = urlencoding::decode(email)
        .map_err(|_| Error::RustError("Invalid email encoding".to_string()))?
        .into_owned();

    let d = ctx.env.d1("DB")?;
    let user = db::users::get_by_email(&d, &email).await?;
    Response::from_json(&LookupUserResult { user })
}

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let users = db::users::list_all(&d).await?;
    Response::from_json(&users)
}

pub async fn upsert(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let body: ManageUserInput = req.json().await?;
    let user = db::users::upsert(&d, &body).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "user",
        &user.id,
        "upsert",
        None,
        Some(&serde_json::to_string(&user).unwrap_or_default()),
    )
    .await;

    api::with_audit_outcome(Response::from_json(&user)?, recorded)
}
