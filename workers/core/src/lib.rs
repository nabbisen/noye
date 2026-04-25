//! Noye Core Worker エントリーポイント。
//!
//! 本ワーカーは `workers_dev = false` かつ独自ルートを設定しないため、
//! 以下の経路からのみ到達可能:
//! - Cloudflare Service Binding 経由 (Gateway Worker から)
//! - Cloudflare Cron Triggers からの `scheduled` イベント
//!
//! さらに `X-Gateway-Token` ヘッダの検証により二重防御を敷く。

use worker::*;

mod api;
mod db;
mod monitor;
mod notify;

/// HTTP リクエストハンドラ (Gateway からの内部 API 呼び出し専用)。
#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        // ── ヘルスチェック ──
        .get("/healthz", |_, _| Response::ok("ok"))
        // ── ユーザー情報照会 (Gateway が認証時に呼び出す) ──
        .get_async("/users/lookup/:email", api::users::lookup)
        // ── 監視対象 ──
        .get_async("/targets", api::targets::list)
        .get_async("/targets/summary", api::targets::summary)
        .get_async("/targets/states", api::targets::states)
        .get_async("/targets/:id", api::targets::get)
        .post_async("/targets", api::targets::create)
        .put_async("/targets/:id", api::targets::update)
        .delete_async("/targets/:id", api::targets::delete)
        .get_async("/targets/:id/state", api::targets::state_for)
        .get_async("/targets/:id/results", api::targets::results)
        // ── インシデント ──
        .get_async("/incidents", api::incidents::list)
        .post_async("/incidents/:id/resolve", api::incidents::resolve)
        // ── メンテナンス ──
        .get_async("/maintenance", api::maintenance::list)
        .post_async("/maintenance", api::maintenance::create)
        // ── 監査ログ ──
        .get_async("/audit", api::audit::list)
        // ── ユーザー管理 ──
        .get_async("/users", api::users::list)
        .post_async("/users", api::users::upsert)
        .run(req, env)
        .await
}

/// Cron Trigger ハンドラ (監視ワーカー, 要件2-4)。
#[event(scheduled)]
pub async fn scheduled(_event: ScheduledEvent, env: Env, _ctx: ScheduleContext) {
    if let Err(e) = monitor::engine::run_scheduled_checks(&env).await {
        console_error!("Scheduled check error: {:?}", e);
    }
}
