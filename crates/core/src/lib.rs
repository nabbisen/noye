//! Noye Core Worker entry point.
//!
//! This worker has `workers_dev = false` and no custom route configured, so
//! it is only reachable through:
//! - Cloudflare Service Bindings (from the Gateway worker)
//! - Cloudflare Cron Triggers (the `scheduled` event)
//!
//! In addition, the `X-Gateway-Token` header is verified for defense in depth.

use worker::*;

mod api;
mod db;
mod env_check;
mod migration;
mod monitor;
mod notify;
mod stats;

/// HTTP request handler (only for internal API calls from the Gateway).
#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Self-check: see crates/gateway/src/env_check.rs for the rationale.
    if let Err(msg) = env_check::check_no_leaked_dev_fallbacks(&env) {
        return Response::error(msg, 500);
    }

    // Schema self-check: refuse to serve a database that predates the
    // audit hash-chain columns, rather than letting every audit insert
    // fail silently later. See rfcs/handoffs/01-migration-applicability.md
    // Build step 4 and db::audit::assert_hash_columns_present.
    let d1 = env.d1("DB")?;
    if let Err(msg) = db::audit::assert_hash_columns_present(&d1).await {
        return Response::error(msg, 500);
    }

    let router = Router::new();

    router
        // ── health check ──
        .get("/healthz", |_, _| Response::ok("ok"))
        // ── User lookup (called by the Gateway during authentication) ──
        .get_async("/users/lookup/:email", api::users::lookup)
        .post_async("/users/resolve-identity", api::users::resolve_identity)
        // ── targets ──
        .get_async("/targets", api::targets::list)
        .get_async("/targets/summary", api::targets::summary)
        .get_async("/targets/states", api::targets::states)
        .get_async("/targets/:id", api::targets::get)
        .post_async("/targets", api::targets::create)
        .put_async("/targets/:id", api::targets::update)
        .delete_async("/targets/:id", api::targets::delete)
        .get_async("/targets/:id/state", api::targets::state_for)
        .get_async("/targets/:id/results", api::targets::results)
        .get_async("/targets/:id/channels", api::channels::list_for_target)
        .post_async("/targets/:id/channels", api::channels::attach)
        .delete_async("/targets/:id/channels/:channel_id", api::channels::detach)
        .get_async("/targets/:id/sla", api::stats::target_sla)
        .get_async("/targets/:id/sla/multi", api::stats::target_sla_multi)
        .get_async(
            "/targets/:id/incidents",
            api::incidents::list_for_target_in_window,
        )
        // ── incidents ──
        .get_async("/incidents", api::incidents::list)
        .post_async("/incidents/:id/resolve", api::incidents::resolve)
        // ── Maintenance ──
        .get_async("/maintenance", api::maintenance::list)
        .post_async("/maintenance", api::maintenance::create)
        // ── notification channels ──
        .get_async("/channels", api::channels::list)
        .post_async("/channels", api::channels::create)
        .get_async("/channels/:id", api::channels::get)
        .put_async("/channels/:id", api::channels::update)
        .delete_async("/channels/:id", api::channels::delete)
        .post_async("/channels/:id/test", api::channels::send_test)
        .get_async("/channels/:id/targets", api::channels::list_targets_for)
        // ── stats / SLA ──
        .get_async("/stats/sla", api::stats::aggregate_sla)
        // ── audit log ──
        .get_async("/audit", api::audit::list)
        .get_async("/audit/verify", api::audit::verify)
        .get_async("/audit/login-history", api::audit::login_history)
        .post_async("/audit/login", api::audit::record_login)
        // ── user management ──
        .get_async("/users", api::users::list)
        .post_async("/users", api::users::upsert)
        // ── configuration migration ──
        .get_async("/admin/migration/export", api::migration::export)
        .post_async("/admin/migration/import", api::migration::import)
        .run(req, env)
        .await
}

/// Cron Trigger handler (monitor worker, requirement 2-4).
#[event(scheduled)]
pub async fn scheduled(event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Err(e) = monitor::engine::run_scheduled_checks(&env, &event).await {
        console_error!("Scheduled check error: {:?}", e);
    }
}
