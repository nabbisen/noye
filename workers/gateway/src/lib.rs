//! Noye Gateway Worker エントリーポイント。
//!
//! 外部リクエスト (ブラウザからのアクセス) を受け付け、OIDC 認証フローを実行し、
//! 認証済みユーザーからのデータ操作を Core ワーカーに Service Binding で橋渡しする。

use worker::*;

mod auth;
mod core_client;
mod ui;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    let router = Router::new();

    router
        .get("/healthz", |_, _| Response::ok("ok"))
        .get_async("/auth/login", handle_auth_login)
        .get_async("/auth/callback", handle_auth_callback)
        .post_async("/auth/logout", handle_auth_logout)
        .get_async("/auth/logout", handle_auth_logout)
        .get_async("/", handle_dashboard)
        .get_async("/targets", handle_targets_list)
        .get_async("/targets/:id", handle_target_detail)
        .post_async("/api/targets", handle_create_target)
        .put_async("/api/targets/:id", handle_update_target)
        .delete_async("/api/targets/:id", handle_delete_target)
        .get_async("/api/targets/:id/results", handle_target_results)
        .get_async("/incidents", handle_incidents_list)
        .post_async("/api/incidents/:id/resolve", handle_resolve_incident)
        .get_async("/maintenance", handle_maintenance_list)
        .post_async("/api/maintenance", handle_create_maintenance)
        .get_async("/audit", handle_audit_log)
        .get_async("/settings", handle_settings)
        .post_async("/api/settings/users", handle_manage_users)
        .run(req, env)
        .await
}

async fn authenticate(req: &Request, env: &Env) -> std::result::Result<auth::Caller, Response> {
    match auth::extract_caller(req, env).await {
        Ok(caller) => Ok(caller),
        Err(e) if auth::is_unauthorized(&e) => {
            let return_to = req.url().map(|u| u.path().to_string()).unwrap_or_else(|_| "/".to_string());
            let loc = format!("/auth/login?return_to={}", urlencoding::encode(&return_to));
            let mut h = Headers::new();
            let _ = h.set("Location", &loc);
            Err(Response::empty()
                .and_then(|r| Ok(r.with_status(302).with_headers(h)))
                .unwrap_or_else(|_| Response::ok("redirecting").unwrap()))
        }
        Err(e) => Err(error_response(403, &format!("{:?}", e))
            .unwrap_or_else(|_| Response::ok("forbidden").unwrap())),
    }
}

async fn handle_dashboard(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    let summary = core_client::status_summary(&ctx.env, &caller).await?;
    let recent_incidents = core_client::list_incidents(&ctx.env, &caller, 10).await?;
    let html = ui::layout::wrap("Dashboard", &caller, &ui::dashboard::render(&summary, &recent_incidents));
    html_response(&html)
}

async fn handle_targets_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    let targets = core_client::list_targets(&ctx.env, &caller).await?;
    let states = core_client::list_states(&ctx.env, &caller).await?;
    let html = ui::layout::wrap("Targets", &caller, &ui::targets::render_list(&targets, &states, &caller));
    html_response(&html)
}

async fn handle_target_detail(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    let id = ctx.param("id").unwrap();

    let target = core_client::get_target(&ctx.env, &caller, id).await?;
    if !auth::rbac::can_view_target(&caller, &target.owner_id) {
        return error_response(403, "Forbidden: not the owner");
    }

    let state = core_client::get_state(&ctx.env, &caller, id).await?;
    let results = core_client::list_results(&ctx.env, &caller, id, 50).await?;
    let html = ui::layout::wrap(&format!("Target: {}", target.name), &caller,
        &ui::targets::render_detail(&target, &state, &results));
    html_response(&html)
}

async fn handle_create_target(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let body: noye_shared::CreateTargetInput = req.json().await?;
    let target = core_client::create_target(&ctx.env, &caller, &body).await?;
    Response::from_json(&target)
}

async fn handle_update_target(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    let body: noye_shared::UpdateTargetInput = req.json().await?;
    let updated = core_client::update_target(&ctx.env, &caller, id, &body).await?;
    Response::from_json(&updated)
}

async fn handle_delete_target(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    core_client::delete_target(&ctx.env, &caller, id).await?;
    Response::ok("deleted")
}

async fn handle_target_results(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    let id = ctx.param("id").unwrap();
    let target = core_client::get_target(&ctx.env, &caller, id).await?;
    if !auth::rbac::can_view_target(&caller, &target.owner_id) {
        return error_response(403, "Forbidden: not the owner");
    }
    let results = core_client::list_results(&ctx.env, &caller, id, 100).await?;
    Response::from_json(&results)
}

async fn handle_incidents_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    let incidents = core_client::list_incidents(&ctx.env, &caller, 100).await?;
    let html = ui::layout::wrap("Incidents", &caller, &ui::incidents::render_list(&incidents));
    html_response(&html)
}

async fn handle_resolve_incident(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    let body: noye_shared::ResolveIncidentInput = req.json().await?;
    core_client::resolve_incident(&ctx.env, &caller, id, &body).await?;
    Response::ok("resolved")
}

async fn handle_maintenance_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    let windows = core_client::list_maintenance(&ctx.env, &caller).await?;
    let html = ui::layout::wrap("Maintenance", &caller, &ui::maintenance::render_list(&windows, &caller));
    html_response(&html)
}

async fn handle_create_maintenance(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let body: noye_shared::CreateMaintenanceInput = req.json().await?;
    let mw = core_client::create_maintenance(&ctx.env, &caller, &body).await?;
    Response::from_json(&mw)
}

async fn handle_audit_log(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let logs = core_client::list_audit(&ctx.env, &caller, 200).await?;
    let html = ui::layout::wrap("Audit Log", &caller, &ui::audit::render_list(&logs));
    html_response(&html)
}

async fn handle_settings(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let users = core_client::list_users(&ctx.env, &caller).await?;
    let html = ui::layout::wrap("Settings", &caller, &ui::settings::render(&users));
    html_response(&html)
}

async fn handle_manage_users(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await { Ok(c) => c, Err(r) => return Ok(r) };
    auth::require_admin(&caller)?;
    let body: noye_shared::ManageUserInput = req.json().await?;
    let user = core_client::upsert_user(&ctx.env, &caller, &body).await?;
    Response::from_json(&user)
}

// ── OIDC 認証フロー ──

async fn handle_auth_login(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let return_to = req.url()?.query_pairs()
        .find(|(k, _)| k == "return_to")
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| "/".to_string());
    let auth_url = auth::oidc::build_authorization_request(&ctx.env, &return_to).await?;
    redirect(&auth_url, &[])
}

async fn handle_auth_callback(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let params: std::collections::HashMap<String, String> = url.query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string())).collect();

    if let Some(err) = params.get("error") {
        let desc = params.get("error_description").cloned().unwrap_or_default();
        return error_response(401, &format!("OIDC error: {} — {}", err, desc));
    }

    let code = params.get("code").ok_or_else(|| Error::RustError("Missing code".to_string()))?;
    let state = params.get("state").ok_or_else(|| Error::RustError("Missing state".to_string()))?;

    let (claims, return_to) = auth::oidc::handle_callback(&ctx.env, code, state).await?;
    let user_email = claims.email.clone()
        .ok_or_else(|| Error::RustError("ID Token has no email claim".to_string()))?;

    let user = core_client::lookup_user(&ctx.env, &user_email).await?;
    let registered = user.as_ref().map(|u| u.is_active).unwrap_or(false);
    if !registered {
        return error_response(403,
            &format!("Forbidden: {} is not a registered Noye user.", user_email));
    }

    let (_session, cookie_value) = auth::session::create(&ctx.env, &user_email, &claims.sub).await?;
    redirect(&return_to, &[cookie_value])
}

async fn handle_auth_logout(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let cookie_name = auth::session::cookie_name(&ctx.env);
    if let Ok(Some(sid)) = auth::cookie::get(&req, &cookie_name) {
        let _ = auth::session::destroy(&ctx.env, &sid).await;
    }
    let clear = auth::session::clear_cookie(&ctx.env);
    let destination = match auth::oidc::end_session_url(&ctx.env).await {
        Ok(Some(url)) => url,
        _ => "/".to_string(),
    };
    redirect(&destination, &[clear])
}

// ── ユーティリティ ──

fn html_response(body: &str) -> Result<Response> {
    let mut headers = Headers::new();
    headers.set("Content-Type", "text/html; charset=utf-8")?;
    Ok(Response::ok(body)?.with_headers(headers))
}

fn redirect(location: &str, set_cookies: &[String]) -> Result<Response> {
    let mut headers = Headers::new();
    headers.set("Location", location)?;
    for c in set_cookies {
        headers.append("Set-Cookie", c)?;
    }
    Ok(Response::empty()?.with_status(302).with_headers(headers))
}

fn error_response(status: u16, message: &str) -> Result<Response> {
    let body = format!(
        r#"<!DOCTYPE html><html lang="ja"><head><meta charset="UTF-8"><title>Error</title></head>
        <body style="font-family:sans-serif;padding:2rem;max-width:40em;margin:0 auto">
        <h1>Error {}</h1><p role="alert">{}</p>
        <p><a href="/auth/login">ログインに戻る</a></p></body></html>"#,
        status, ui::layout::escape_html(message)
    );
    let mut headers = Headers::new();
    headers.set("Content-Type", "text/html; charset=utf-8")?;
    Ok(Response::from_html(body)?.with_status(status).with_headers(headers))
}
