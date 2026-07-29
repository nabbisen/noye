//! Noye Gateway Worker entry point.
//!
//! Receives external requests (from browsers), runs the OIDC authentication flow, and
//! forwards data operations from authenticated users to the Core worker via a Service Binding.

use worker::*;

mod auth;
mod core_client;
mod csv_export;
mod env_check;
mod rate_limit;
mod safe_redirect;
mod security_headers;
mod ui;

#[event(fetch)]
pub async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // Self-check: if NOYE_ENV is unset (production) and any well-known
    // dev-fallback value is still in [vars], fail closed before serving the
    // request. This catches deploys that forgot `wrangler secret put`.
    if let Err(msg) = env_check::check_no_leaked_dev_fallbacks(&env) {
        return error_response(500, &msg);
    }

    let router = Router::new();

    router
        .get("/healthz", |_, _| {
            with_security_headers(Response::ok("ok")?)
        })
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
        .post_async("/api/targets/:id/channels", handle_attach_channel_to_target)
        .delete_async(
            "/api/targets/:id/channels/:channel_id",
            handle_detach_channel_from_target,
        )
        .get_async("/incidents", handle_incidents_list)
        .post_async("/api/incidents/:id/resolve", handle_resolve_incident)
        .get_async("/maintenance", handle_maintenance_list)
        .post_async("/api/maintenance", handle_create_maintenance)
        .get_async("/channels", handle_channels_list)
        .get_async("/channels/:id", handle_channel_detail)
        .post_async("/api/channels", handle_create_channel)
        .put_async("/api/channels/:id", handle_update_channel)
        .delete_async("/api/channels/:id", handle_delete_channel)
        .post_async("/api/channels/:id/test", handle_test_channel)
        .get_async("/stats", handle_stats_page)
        .get_async("/stats/:id", handle_stats_detail)
        .get_async("/api/stats/sla", handle_stats_json)
        .get_async("/api/stats/sla.csv", handle_stats_csv)
        .get_async("/api/stats/incidents/:id.csv", handle_incidents_csv)
        .get_async("/audit", handle_audit_log)
        .get_async("/api/admin/audit/verify", handle_audit_verify)
        .get_async("/me/security", handle_me_security)
        .post_async("/api/me/sessions/revoke-others", handle_me_revoke_others)
        .get_async("/settings", handle_settings)
        .post_async("/api/settings/users", handle_manage_users)
        .get_async("/admin/migration", handle_migration_page)
        .get_async("/api/admin/migration/export", handle_migration_export)
        .post_async("/api/admin/migration/import", handle_migration_import)
        .run(req, env)
        .await
}

async fn authenticate(req: &Request, env: &Env) -> std::result::Result<auth::Caller, Response> {
    match auth::extract_caller(req, env).await {
        Ok(caller) => Ok(caller),
        Err(e) if auth::is_unauthorized(&e) => {
            // The path() of the current request URL, which is by construction
            // a same-origin path. Sanitize anyway as a self-documenting
            // assertion — `is_safe_return_to` is a constant-time string check.
            let return_to = req
                .url()
                .map(|u| u.path().to_string())
                .ok()
                .map(|p| safe_redirect::sanitize_return_to(&p))
                .unwrap_or_else(|| "/".to_string());
            let loc = format!("/auth/login?return_to={}", urlencoding::encode(&return_to));
            let h = Headers::new();
            let _ = h.set("Location", &loc);
            let _ = security_headers::apply(&h);
            Err(Response::empty()
                .map(|r| r.with_status(302).with_headers(h))
                .unwrap_or_else(|_| Response::ok("redirecting").unwrap()))
        }
        Err(e) => Err(error_response(403, &format!("{:?}", e))
            .unwrap_or_else(|_| Response::ok("forbidden").unwrap())),
    }
}

async fn handle_dashboard(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let summary = core_client::status_summary(&ctx.env, &caller).await?;
    let recent_incidents = core_client::list_incidents(&ctx.env, &caller, 10).await?;
    let html = ui::layout::wrap(
        "Dashboard",
        &caller,
        csrf.as_deref(),
        &ui::dashboard::render(&summary, &recent_incidents),
    );
    html_response(&html)
}

async fn handle_targets_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let targets = core_client::list_targets(&ctx.env, &caller).await?;
    let states = core_client::list_states(&ctx.env, &caller).await?;
    let html = ui::layout::wrap(
        "Targets",
        &caller,
        csrf.as_deref(),
        &ui::targets::render_list(&targets, &states, &caller),
    );
    html_response(&html)
}

async fn handle_target_detail(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let id = ctx.param("id").unwrap();

    let target = core_client::get_target(&ctx.env, &caller, id).await?;
    if !auth::rbac::can_view_target(&caller, &target.owner_id) {
        return error_response(403, "Forbidden: not the owner");
    }

    let active_tab = parse_target_tab(&req);
    let state = core_client::get_state(&ctx.env, &caller, id).await?;

    // Skip the results fetch when the active tab doesn't show them. The
    // results query is the heaviest of the three (50 rows) and Phase C's
    // tabs let us avoid it on Overview / Channels / Settings.
    let results = if active_tab == ui::targets::TargetTab::Results {
        core_client::list_results(&ctx.env, &caller, id, 50).await?
    } else {
        Vec::new()
    };

    let mut body = ui::targets::render_detail(&target, &state, &results, active_tab);

    // Channel attachments are only fetched and rendered on the Channels
    // tab. Admins additionally get the attach/detach controls.
    if active_tab == ui::targets::TargetTab::Channels {
        let attached = core_client::list_channels_for_target(&ctx.env, &caller, id).await?;
        let available = if caller.is_admin() {
            core_client::list_channels(&ctx.env, &caller).await?
        } else {
            Vec::new()
        };
        body.push_str(&ui::channels::render_target_attachments(
            id, &attached, &available, &caller,
        ));
    }

    let html = ui::layout::wrap(
        &format!("Target: {}", target.name),
        &caller,
        csrf.as_deref(),
        &body,
    );
    html_response(&html)
}

/// Read the `?tab=` query parameter for /targets/:id. Unknown values
/// fall back to Overview via [`ui::targets::TargetTab::parse`].
fn parse_target_tab(req: &Request) -> ui::targets::TargetTab {
    let url = match req.url() {
        Ok(u) => u,
        Err(_) => return ui::targets::TargetTab::Overview,
    };
    let raw = url
        .query_pairs()
        .find(|(k, _)| k == "tab")
        .map(|(_, v)| v.to_string())
        .unwrap_or_default();
    ui::targets::TargetTab::parse(&raw)
}

async fn handle_create_target(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let body: noye_shared::CreateTargetInput = req.json().await?;
    let target = core_client::create_target(&ctx.env, &caller, &body).await?;
    with_security_headers(Response::from_json(&target)?)
}

async fn handle_update_target(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    let body: noye_shared::UpdateTargetInput = req.json().await?;
    let updated = core_client::update_target(&ctx.env, &caller, id, &body).await?;
    with_security_headers(Response::from_json(&updated)?)
}

async fn handle_delete_target(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    core_client::delete_target(&ctx.env, &caller, id).await?;
    with_security_headers(Response::ok("deleted")?)
}

async fn handle_target_results(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let id = ctx.param("id").unwrap();
    let target = core_client::get_target(&ctx.env, &caller, id).await?;
    if !auth::rbac::can_view_target(&caller, &target.owner_id) {
        return error_response(403, "Forbidden: not the owner");
    }
    let results = core_client::list_results(&ctx.env, &caller, id, 100).await?;
    with_security_headers(Response::from_json(&results)?)
}

async fn handle_incidents_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let incidents = core_client::list_incidents(&ctx.env, &caller, 100).await?;
    let html = ui::layout::wrap(
        "Incidents",
        &caller,
        csrf.as_deref(),
        &ui::incidents::render_list(&incidents),
    );
    html_response(&html)
}

async fn handle_resolve_incident(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    let body: noye_shared::ResolveIncidentInput = req.json().await?;
    core_client::resolve_incident(&ctx.env, &caller, id, &body).await?;
    with_security_headers(Response::ok("resolved")?)
}

async fn handle_maintenance_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let windows = core_client::list_maintenance(&ctx.env, &caller).await?;
    let html = ui::layout::wrap(
        "Maintenance",
        &caller,
        csrf.as_deref(),
        &ui::maintenance::render_list(&windows, &caller),
    );
    html_response(&html)
}

async fn handle_create_maintenance(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let body: noye_shared::CreateMaintenanceInput = req.json().await?;
    let mw = core_client::create_maintenance(&ctx.env, &caller, &body).await?;
    with_security_headers(Response::from_json(&mw)?)
}

async fn handle_audit_log(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    auth::require_admin(&caller)?;
    let logs = core_client::list_audit(&ctx.env, &caller, 200).await?;
    let html = ui::layout::wrap(
        "Audit Log",
        &caller,
        csrf.as_deref(),
        &ui::audit::render_list(&logs),
    );
    html_response(&html)
}

/// JSON endpoint exposing the Core's hash-chain integrity check over the
/// audit log. Admin-only. Returned as `application/json` for direct
/// consumption (`curl ... | jq`); a UI surface is intentionally deferred to
/// Phase 3 alongside `/me/security`.
async fn handle_audit_verify(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    auth::require_admin(&caller)?;
    let report = core_client::verify_audit_chain(&ctx.env, &caller).await?;
    with_security_headers(Response::from_json(&report)?)
}

// ── Personal security page ──

async fn handle_me_security(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;

    // Current session (we already loaded it in authenticate via the
    // session cookie, but extract_caller doesn't surface it; load again).
    let current = auth::session::load_from_cookie(&req, &ctx.env)
        .await
        .ok()
        .flatten();

    // All active sessions belonging to this user (best-effort).
    let all_sessions = auth::session::list_active_for_user(&ctx.env, &caller.email)
        .await
        .unwrap_or_default();

    // Login history (best-effort — surface empty list if Core fails).
    let history = core_client::login_history(&ctx.env, &caller, 20)
        .await
        .unwrap_or_default();

    let body = ui::me::render(
        &caller,
        current.as_ref(),
        &all_sessions,
        &history,
        caller.is_admin(),
    );
    let html = ui::layout::wrap("Security", &caller, csrf.as_deref(), &body);
    html_response(&html)
}

async fn handle_me_revoke_others(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }

    // Identify the current session so we exclude it from the revoke list —
    // calling this from a session shouldn't terminate that session itself.
    let current = match auth::session::load_from_cookie(&req, &ctx.env).await? {
        Some(s) => s,
        None => return error_response(403, "no active session"),
    };

    let revoked =
        auth::session::revoke_others_for_user(&ctx.env, &caller.email, &current.session_id).await?;

    let body = serde_json::json!({"revoked": revoked});
    with_security_headers(Response::from_json(&body)?)
}

/// Read the `?window=` query parameter and clamp it to a safe known value.
/// Anything unrecognized falls back to "24h" so a malformed URL still renders.
fn parse_stats_window(req: &Request) -> String {
    let url = match req.url() {
        Ok(u) => u,
        Err(_) => return "24h".to_string(),
    };
    let raw = url
        .query_pairs()
        .find(|(k, _)| k == "window")
        .map(|(_, v)| v.to_string());
    match raw.as_deref() {
        Some(w @ ("24h" | "7d" | "30d" | "90d")) => w.to_string(),
        _ => "24h".to_string(),
    }
}

async fn handle_stats_page(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let window = parse_stats_window(&req);
    let summary = core_client::get_aggregate_sla(&ctx.env, &caller, &window).await?;
    let html = ui::layout::wrap(
        "Stats",
        &caller,
        csrf.as_deref(),
        &ui::stats::render_page(&summary, &window, &caller),
    );
    html_response(&html)
}

async fn handle_stats_json(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let window = parse_stats_window(&req);
    let summary = core_client::get_aggregate_sla(&ctx.env, &caller, &window).await?;
    with_security_headers(Response::from_json(&summary)?)
}

async fn handle_stats_csv(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let window = parse_stats_window(&req);
    let summary = core_client::get_aggregate_sla(&ctx.env, &caller, &window).await?;

    let bytes = csv_export::encode_sla_summary(&summary);
    let date = chrono::Utc::now().format("%Y%m%d").to_string();
    let filename = csv_export::build_filename(&format!("sla-{}", window), &date);

    let mut response = Response::from_bytes(bytes)?;
    let headers = response.headers_mut();
    // text/csv with explicit utf-8 charset; the BOM in the body is what makes
    // Excel respect this, but stating the charset is still good hygiene.
    headers.set("Content-Type", "text/csv; charset=utf-8")?;
    headers.set(
        "Content-Disposition",
        &format!(r#"attachment; filename="{}""#, filename),
    )?;
    security_headers::apply(headers)?;
    Ok(response)
}

async fn handle_incidents_csv(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let id = ctx.param("id").unwrap();

    // Same RBAC as the rest of the per-target surface.
    let target = core_client::get_target(&ctx.env, &caller, id).await?;
    if !auth::rbac::can_view_target(&caller, &target.owner_id) {
        return error_response(403, "Forbidden: not the owner of this target");
    }

    let window = parse_stats_window(&req);
    let incidents =
        core_client::list_target_incidents_in_window(&ctx.env, &caller, id, &window).await?;

    let bytes = csv_export::encode_incidents(&incidents);
    let date = chrono::Utc::now().format("%Y%m%d").to_string();
    // Slugify the target id so the filename is filesystem-friendly even if
    // the id has unusual characters. This is best-effort; the
    // Content-Disposition value is also quoted to handle edge cases.
    let safe_id: String = id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let filename = csv_export::build_filename(&format!("incidents-{}-{}", safe_id, window), &date);

    let mut response = Response::from_bytes(bytes)?;
    let headers = response.headers_mut();
    headers.set("Content-Type", "text/csv; charset=utf-8")?;
    headers.set(
        "Content-Disposition",
        &format!(r#"attachment; filename="{}""#, filename),
    )?;
    security_headers::apply(headers)?;
    Ok(response)
}

async fn handle_stats_detail(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let id = ctx.param("id").unwrap();

    // Same RBAC as the rest of the target surfaces. Member sees only their
    // own; admin sees everything.
    let target = core_client::get_target(&ctx.env, &caller, id).await?;
    if !auth::rbac::can_view_target(&caller, &target.owner_id) {
        return error_response(403, "Forbidden: not the owner of this target");
    }

    let window = parse_stats_window(&req);
    let selected_report = core_client::get_target_sla(&ctx.env, &caller, id, &window).await?;
    let multi = core_client::get_target_sla_multi(&ctx.env, &caller, id).await?;
    let incidents =
        core_client::list_target_incidents_in_window(&ctx.env, &caller, id, &window).await?;

    let title = format!("Stats: {}", target.name);
    let html = ui::layout::wrap(
        &title,
        &caller,
        csrf.as_deref(),
        &ui::stats::render_detail(&target, &window, &selected_report, &multi, &incidents),
    );
    html_response(&html)
}

async fn handle_settings(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    auth::require_admin(&caller)?;
    let users = core_client::list_users(&ctx.env, &caller).await?;
    let html = ui::layout::wrap(
        "Settings",
        &caller,
        csrf.as_deref(),
        &ui::settings::render(&users),
    );
    html_response(&html)
}

async fn handle_manage_users(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let body: noye_shared::ManageUserInput = req.json().await?;
    let user = core_client::upsert_user(&ctx.env, &caller, &body).await?;
    with_security_headers(Response::from_json(&user)?)
}

// ── Configuration migration ──

async fn handle_migration_page(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    auth::require_admin(&caller)?;
    let html = ui::layout::wrap(
        "Configuration migration",
        &caller,
        csrf.as_deref(),
        &ui::migration::render_page(&caller),
    );
    html_response(&html)
}

async fn handle_migration_export(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    auth::require_admin(&caller)?;

    let include_users = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "include_users")
        .map(|(_, v)| v == "true" || v == "1")
        .unwrap_or(false);

    let payload = core_client::export_migration(&ctx.env, &caller, include_users).await?;
    let body = serde_json::to_string_pretty(&payload)
        .map_err(|e| Error::RustError(format!("serialize: {}", e)))?;

    // Suggest a download filename. The browser side also derives one from
    // the current date, but Content-Disposition makes scripts/curl users
    // get a sensible default too.
    let date = chrono::Utc::now().format("%Y%m%d").to_string();
    let filename = format!("noye-export-{}.json", date);

    let mut response = Response::ok(body)?;
    let headers = response.headers_mut();
    headers.set("Content-Type", "application/json")?;
    headers.set(
        "Content-Disposition",
        &format!(r#"attachment; filename="{}""#, filename),
    )?;
    security_headers::apply(headers)?;
    Ok(response)
}

async fn handle_migration_import(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let body: noye_shared::ImportRequest = req.json().await?;
    let result = core_client::import_migration(&ctx.env, &caller, &body).await?;
    with_security_headers(Response::from_json(&result)?)
}

// ── Notification channels ──

async fn handle_channels_list(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let channels = core_client::list_channels(&ctx.env, &caller).await?;
    let html = ui::layout::wrap(
        "Notification channels",
        &caller,
        csrf.as_deref(),
        &ui::channels::render_list(&channels, &caller),
    );
    html_response(&html)
}

async fn handle_channel_detail(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    let csrf = current_csrf_token(&req, &ctx.env).await;
    let id = ctx.param("id").unwrap();

    let channel = core_client::get_channel(&ctx.env, &caller, id).await?;

    // RBAC mirrors targets: members may only view channels they own; admins
    // see everything.
    if !caller.is_admin() && caller.user_id != channel.owner_id {
        return error_response(403, "Forbidden: not the owner of this channel");
    }

    // Reverse-lookup is admin-only — non-admins never see a channel they
    // don't own (the check above), and even those they own typically have
    // no attached targets unless an admin attached them. We still surface
    // the section to admins always.
    let attached = if caller.is_admin() {
        core_client::list_targets_for_channel(&ctx.env, &caller, id).await?
    } else {
        Vec::new()
    };

    let html = ui::layout::wrap(
        &format!("Channel: {}", channel.name),
        &caller,
        csrf.as_deref(),
        &ui::channels::render_detail(&channel, &attached, &caller),
    );
    html_response(&html)
}

async fn handle_create_channel(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let body: noye_shared::CreateNotificationChannelInput = req.json().await?;
    let channel = core_client::create_channel(&ctx.env, &caller, &body).await?;
    with_security_headers(Response::from_json(&channel)?)
}

async fn handle_update_channel(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    let body: noye_shared::UpdateNotificationChannelInput = req.json().await?;
    let channel = core_client::update_channel(&ctx.env, &caller, id, &body).await?;
    with_security_headers(Response::from_json(&channel)?)
}

async fn handle_delete_channel(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();
    core_client::delete_channel(&ctx.env, &caller, id).await?;
    with_security_headers(Response::ok("deleted")?)
}

async fn handle_test_channel(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let id = ctx.param("id").unwrap();

    // Rate-limit before crossing the Service Binding so abusive bursts never
    // touch the Core or the upstream notification endpoint.
    match rate_limit::check_and_consume(&ctx.env, id).await? {
        rate_limit::Decision::Allowed => {}
        rate_limit::Decision::Denied {
            scope,
            retry_after_sec,
        } => {
            let body = format!(
                "Rate limit exceeded ({}). Try again in {} second{}.",
                scope.as_str(),
                retry_after_sec,
                if retry_after_sec == 1 { "" } else { "s" }
            );
            let headers = Headers::new();
            headers.set("Retry-After", &retry_after_sec.to_string())?;
            headers.set("Content-Type", "text/plain; charset=utf-8")?;
            security_headers::apply(&headers)?;
            return Ok(Response::error(body, 429)?.with_headers(headers));
        }
    }

    core_client::test_channel(&ctx.env, &caller, id).await?;
    with_security_headers(Response::ok("sent")?)
}

async fn handle_attach_channel_to_target(
    mut req: Request,
    ctx: RouteContext<()>,
) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let target_id = ctx.param("id").unwrap();
    let body: noye_shared::AttachChannelInput = req.json().await?;
    core_client::attach_channel(&ctx.env, &caller, target_id, &body).await?;
    with_security_headers(Response::ok("attached")?)
}

async fn handle_detach_channel_from_target(
    req: Request,
    ctx: RouteContext<()>,
) -> Result<Response> {
    let caller = match authenticate(&req, &ctx.env).await {
        Ok(c) => c,
        Err(r) => return Ok(r),
    };
    if let Err(r) = verify_csrf(&req, &ctx.env).await {
        return Ok(r);
    }
    auth::require_admin(&caller)?;
    let target_id = ctx.param("id").unwrap();
    let channel_id = ctx.param("channel_id").unwrap();
    core_client::detach_channel(&ctx.env, &caller, target_id, channel_id).await?;
    with_security_headers(Response::ok("detached")?)
}

// ── OIDC Authenticationフロー ──

async fn handle_auth_login(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // Rate limit by client IP BEFORE any KV write to pending_login: the entire
    // point is to prevent unauthenticated traffic from filling KV. Denials
    // return 429 with Retry-After; the user can retry after the window resets.
    let ip = client_ip(&req);
    match rate_limit::check_and_consume_login(&ctx.env, &ip).await? {
        rate_limit::Decision::Allowed => {}
        rate_limit::Decision::Denied {
            scope,
            retry_after_sec,
        } => {
            let body = format!(
                "Too many login attempts ({} limit). Try again in {} seconds.",
                scope.as_str(),
                retry_after_sec
            );
            let headers = Headers::new();
            headers.set("Retry-After", &retry_after_sec.to_string())?;
            headers.set("Content-Type", "text/plain; charset=utf-8")?;
            security_headers::apply(&headers)?;
            return Ok(Response::error(body, 429)?.with_headers(headers));
        }
    }

    let raw_return_to = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "return_to")
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| "/".to_string());
    // Open-redirect guard: only same-origin path-relative URLs are permitted.
    // Anything off-origin (or weirdly shaped) is rewritten to the dashboard.
    let return_to = safe_redirect::sanitize_return_to(&raw_return_to);
    let auth_url = auth::oidc::build_authorization_request(&ctx.env, &return_to).await?;
    redirect(&auth_url, &[])
}

async fn handle_auth_callback(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let url = req.url()?;
    let params: std::collections::HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    if let Some(err) = params.get("error") {
        let desc = params.get("error_description").cloned().unwrap_or_default();
        return error_response(401, &format!("OIDC error: {} — {}", err, desc));
    }

    let code = params
        .get("code")
        .ok_or_else(|| Error::RustError("Missing code".to_string()))?;
    let state = params
        .get("state")
        .ok_or_else(|| Error::RustError("Missing state".to_string()))?;

    let (claims, return_to) = auth::oidc::handle_callback(&ctx.env, code, state).await?;
    let user_email = claims
        .email
        .clone()
        .ok_or_else(|| Error::RustError("ID Token has no email claim".to_string()))?;

    let user = core_client::lookup_user(&ctx.env, &user_email).await?;
    let registered = user.as_ref().map(|u| u.is_active).unwrap_or(false);
    if !registered {
        return error_response(
            403,
            &format!("Forbidden: {} is not a registered Noye user.", user_email),
        );
    }
    let user_id = user.as_ref().map(|u| u.id.clone()).unwrap_or_default();

    let (_session, cookie_value) =
        auth::session::create(&ctx.env, &user_email, &claims.sub).await?;

    // Best-effort: record this login in the audit log so it shows up on
    // /me/security. Failures here do not block the login (the session is
    // already created); the proxy logs to console for diagnosis.
    let ip = client_ip(&req);
    let _ = core_client::record_login(&ctx.env, &user_id, &user_email, Some(&ip)).await;

    // Sanitize again on the way out: the value comes from KV which we wrote
    // ourselves, but defense in depth costs nothing and protects against any
    // future code path that lets `pending.return_to` originate elsewhere.
    let safe_return = safe_redirect::sanitize_return_to(&return_to);
    redirect(&safe_return, &[cookie_value])
}

async fn handle_auth_logout(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    // POST logout requires the CSRF token (defends against a `<form action>`
    // attack from another origin that would otherwise force-logout the user
    // — low harm, but trivially blocked). GET logout is left unguarded so a
    // plain `<a href="/auth/logout">` link works for the UX. Note that the
    // attacker still cannot impersonate the user here; the worst they can
    // do is end the session.
    if req.method() == Method::Post
        && let Err(r) = verify_csrf(&req, &ctx.env).await
    {
        return Ok(r);
    }
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

// ── Utilities ──

/// Read the originating client IP from the `CF-Connecting-IP` header.
///
/// Cloudflare sets this at the edge; it is **trusted** by the worker because
/// only Cloudflare can reach the worker. When the header is absent (e.g.
/// `wrangler dev` invoked from a terminal that bypasses the edge), we fall
/// back to the literal string `"unknown"` — this keeps every such caller in
/// the same rate-limit bucket, which is the conservative behavior.
fn client_ip(req: &Request) -> String {
    req.headers()
        .get("CF-Connecting-IP")
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string())
}

/// Read the current session's CSRF token, returning `None` if the session
/// is missing the field (legacy session pre-CSRF rollout) or if the session
/// itself is gone.
///
/// Used by the layout renderer to embed a `<meta name="csrf-token">` tag.
async fn current_csrf_token(req: &Request, env: &Env) -> Option<String> {
    auth::session::load_from_cookie(req, env)
        .await
        .ok()
        .flatten()
        .and_then(|s| s.csrf_token)
}

/// Verify the `X-CSRF-Token` request header against the session's stored
/// CSRF token, in constant time.
///
/// This is called by every state-changing handler immediately after
/// `authenticate()`. Failure modes:
///
/// - **No header**: 403, "CSRF token missing"
/// - **Header malformed** (wrong length, bad chars): 403 (rejected by
///   `looks_well_formed` before any KV read)
/// - **No session csrf_token** (legacy session pre-rollout): allow the
///   request through with a console warning, so existing logged-in users
///   are not locked out by the deploy. The token will appear on their
///   next session, and any new session created from that point on will
///   enforce CSRF strictly.
/// - **Mismatch**: 403, "CSRF token mismatch"
async fn verify_csrf(req: &Request, env: &Env) -> std::result::Result<(), Response> {
    let presented = req
        .headers()
        .get("X-CSRF-Token")
        .ok()
        .flatten()
        .unwrap_or_default();

    if presented.is_empty() {
        return Err(error_response(403, "CSRF token missing")
            .unwrap_or_else(|_| Response::ok("forbidden").unwrap()));
    }
    if !auth::csrf::looks_well_formed(&presented) {
        return Err(error_response(403, "CSRF token malformed")
            .unwrap_or_else(|_| Response::ok("forbidden").unwrap()));
    }

    let session = match auth::session::load_from_cookie(req, env).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err(error_response(403, "CSRF check requires an active session")
                .unwrap_or_else(|_| Response::ok("forbidden").unwrap()));
        }
        Err(_) => {
            return Err(error_response(403, "CSRF check could not load session")
                .unwrap_or_else(|_| Response::ok("forbidden").unwrap()));
        }
    };

    let stored = match session.csrf_token {
        Some(t) => t,
        None => {
            // Legacy session pre-CSRF rollout. We could fail closed here,
            // but that locks out every user already logged in at deploy
            // time. Allow once with a console warning — the next
            // session::create() call will issue a token.
            console_log!(
                "[csrf] session for {} has no token (legacy); allowing this request",
                session.user_email
            );
            return Ok(());
        }
    };

    if !auth::csrf::constant_time_eq(&presented, &stored) {
        return Err(error_response(403, "CSRF token mismatch")
            .unwrap_or_else(|_| Response::ok("forbidden").unwrap()));
    }
    Ok(())
}

fn html_response(body: &str) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Content-Type", "text/html; charset=utf-8")?;
    security_headers::apply(&headers)?;
    Ok(Response::ok(body)?.with_headers(headers))
}

/// Wrap a Response with the standard security headers in place of writing
/// them by hand. Used for `Response::from_json`, `Response::ok("sent")`,
/// and other simple API responses where we want the same baseline policy
/// as HTML pages.
fn with_security_headers(resp: Response) -> Result<Response> {
    let headers = resp.headers().clone();
    security_headers::apply(&headers)?;
    Ok(resp.with_headers(headers))
}

fn redirect(location: &str, set_cookies: &[String]) -> Result<Response> {
    let headers = Headers::new();
    headers.set("Location", location)?;
    for c in set_cookies {
        headers.append("Set-Cookie", c)?;
    }
    security_headers::apply(&headers)?;
    Ok(Response::empty()?.with_status(302).with_headers(headers))
}

fn error_response(status: u16, message: &str) -> Result<Response> {
    let body = format!(
        r#"<!DOCTYPE html><html lang="en"><head><meta charset="UTF-8"><title>Error</title></head>
        <body style="font-family:sans-serif;padding:2rem;max-width:40em;margin:0 auto">
        <h1>Error {}</h1><p role="alert">{}</p>
        <p><a href="/auth/login">Back to sign in</a></p></body></html>"#,
        status,
        ui::layout::escape_html(message)
    );
    let headers = Headers::new();
    headers.set("Content-Type", "text/html; charset=utf-8")?;
    security_headers::apply(&headers)?;
    Ok(Response::from_html(body)?
        .with_status(status)
        .with_headers(headers))
}
