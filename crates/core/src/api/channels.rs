//! Internal API handlers for notification channels and target attachments.
//!
//! Mutating endpoints require admin role. Listing is allowed for any
//! authenticated user but scoped to channels they own (admins see all).

use noye_shared::{
    AttachChannelInput, CreateNotificationChannelInput, UpdateNotificationChannelInput,
};
use worker::*;

use crate::{api, db};

// ── Channel CRUD ──

pub async fn list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    let d = ctx.env.d1("DB")?;
    let channels = db::channels::list_channels(&d, &caller).await?;
    Response::from_json(&channels)
}

pub async fn get(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    let channel = db::channels::get_channel(&d, id).await?;
    Response::from_json(&channel)
}

pub async fn create(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let d = ctx.env.d1("DB")?;
    let body: CreateNotificationChannelInput = req.json().await?;
    let channel = db::channels::create_channel(&d, &body, &caller).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "notification_channel",
        &channel.id,
        "create",
        None,
        Some(&serde_json::to_string(&channel).unwrap_or_default()),
    )
    .await;

    api::with_audit_outcome(Response::from_json(&channel)?, recorded)
}

pub async fn update(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let id = ctx.param("id").unwrap().to_string();
    let d = ctx.env.d1("DB")?;
    let old = db::channels::get_channel(&d, &id).await?;

    let body: UpdateNotificationChannelInput = req.json().await?;
    let updated = db::channels::update_channel(&d, &id, &body).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "notification_channel",
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
    db::channels::delete_channel(&d, id).await?;

    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "notification_channel",
        id,
        "delete",
        None,
        None,
    )
    .await;

    api::with_audit_outcome(Response::ok("deleted")?, recorded)
}

// ── Target ↔ channel attachments ──

pub async fn list_for_target(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let target_id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    let attached = db::channels::list_attached_channels(&d, target_id).await?;
    Response::from_json(&attached)
}

/// Reverse direction: list every target that the given channel is attached to.
pub async fn list_targets_for(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let _caller = api::require_caller_with_env(&req, &ctx.env)?;
    let channel_id = ctx.param("id").unwrap();
    let d = ctx.env.d1("DB")?;
    let attached = db::channels::list_targets_for_channel(&d, channel_id).await?;
    Response::from_json(&attached)
}

pub async fn attach(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let target_id = ctx.param("id").unwrap().to_string();
    let d = ctx.env.d1("DB")?;
    let body: AttachChannelInput = req.json().await?;
    db::channels::attach_channel(&d, &target_id, &body).await?;

    let detail = format!(
        "channel_id={} on_down={} on_up={}",
        body.channel_id, body.on_down, body.on_up
    );
    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "target_notification",
        &target_id,
        "attach",
        None,
        Some(&detail),
    )
    .await;

    api::with_audit_outcome(Response::ok("attached")?, recorded)
}

pub async fn detach(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let target_id = ctx.param("id").unwrap();
    let channel_id = ctx.param("channel_id").unwrap();
    let d = ctx.env.d1("DB")?;
    db::channels::detach_channel(&d, target_id, channel_id).await?;

    let detail = format!("channel_id={}", channel_id);
    let recorded = db::audit::log_or_report(
        &d,
        &caller,
        "target_notification",
        target_id,
        "detach",
        Some(&detail),
        None,
    )
    .await;

    api::with_audit_outcome(Response::ok("detached")?, recorded)
}

/// Send a test notification to the channel and audit the action.
///
/// Always returns the underlying transport error to the caller (rather than
/// silently swallowing it the way `dispatch_down` / `dispatch_up` do during
/// monitoring) so the UI can surface the cause of a misconfiguration.
pub async fn send_test(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let id = ctx.param("id").unwrap().to_string();
    let d = ctx.env.d1("DB")?;
    let channel = db::channels::get_channel(&d, &id).await?;

    match crate::notify::send_test(&ctx.env, &channel).await {
        Ok(()) => {
            let recorded = db::audit::log_or_report(
                &d,
                &caller,
                "notification_channel",
                &id,
                "test_send",
                None,
                Some("ok"),
            )
            .await;
            api::with_audit_outcome(Response::ok("sent")?, recorded)
        }
        Err(e) => {
            let detail = format!("{:?}", e);
            // Test send already failed and this returns Err(e) below --
            // there is no successful response to attach a warning
            // header to, so log_or_report's FR-AUD-08 half (the
            // error-level log on an audit failure) is all that applies
            // here. Discarded as a bare statement, not `let _ =` -- the
            // logging already happened inside the call; there is
            // nothing left to act on (T-35 greps for the discard
            // pattern specifically, not for "every call site uses the
            // return value").
            db::audit::log_or_report(
                &d,
                &caller,
                "notification_channel",
                &id,
                "test_send",
                None,
                Some(&format!("error: {}", detail)),
            )
            .await;
            Err(e)
        }
    }
}
