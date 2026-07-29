//! Internal API for configuration export and import.
//!
//! Both endpoints are admin-only and audit-logged. Export streams the entire
//! configuration set (minus the optional `users` table) as a single JSON
//! response; import takes the symmetric payload and replays it into D1.
//!
//! ## Why these are not on `/api/admin/*`
//!
//! The Core's API surface is internal-only — every call comes via the
//! Service Binding from the Gateway. The Gateway's external surface uses
//! `/api/admin/...` to make the namespace clear to operators. On the Core we
//! drop the prefix to match the rest of this module's flat layout.

use noye_shared::{ImportRequest, ImportResult, MIGRATION_SCHEMA_VERSION, MigrationExport};
use worker::*;

use crate::{api, db, migration};

/// `GET /admin/migration/export?include_users=true|false`
pub async fn export(req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let include_users = req
        .url()?
        .query_pairs()
        .find(|(k, _)| k == "include_users")
        .map(|(_, v)| v == "true" || v == "1")
        .unwrap_or(false);

    let d = ctx.env.d1("DB")?;
    let data = db::migration::export_all(&d, include_users).await?;

    let payload = MigrationExport {
        schema_version: MIGRATION_SCHEMA_VERSION,
        exported_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        source_deployment: ctx.env.var("DEPLOYMENT_LABEL").ok().map(|v| v.to_string()),
        data,
    };

    let detail = format!(
        "include_users={} targets={} channels={}",
        include_users,
        payload.data.targets.len(),
        payload.data.channels.len(),
    );
    let _ = db::audit::log(
        &d,
        &caller,
        "migration",
        "export",
        "export",
        None,
        Some(&detail),
    )
    .await;

    Response::from_json(&payload)
}

/// `POST /admin/migration/import` — body is an `ImportRequest`. When
/// `apply = false` the validation runs and counts are returned without any
/// D1 write. When `apply = true` the writes happen under the configured
/// conflict policy and the same counts come back populated for real.
pub async fn import(mut req: Request, ctx: RouteContext<()>) -> Result<Response> {
    let caller = api::require_caller_with_env(&req, &ctx.env)?;
    api::require_admin(&caller)?;

    let body: ImportRequest = req.json().await?;

    // Validation runs unconditionally — even an applying import gets a
    // structural check first so we don't half-write an inconsistent payload.
    let validation = migration::validate(&body.payload);
    if !validation.is_ok() {
        let errors = validation.into_errors();
        return Err(Error::RustError(format!(
            "Import payload failed validation:\n  - {}",
            errors.join("\n  - ")
        )));
    }
    let warnings = validation.into_warnings();

    let d = ctx.env.d1("DB")?;

    if !body.apply {
        // Dry-run: report counts as if every row would be inserted. Conflict
        // resolution is not predicted because that would require reading
        // every PK from D1; the operator can re-run with apply=true to see
        // the actual breakdown.
        let (t, c, l, m, u) = migration::count_rows(&body.payload);
        let result = ImportResult {
            applied: false,
            schema_version: MIGRATION_SCHEMA_VERSION,
            conflict_policy: body.on_conflict,
            rows: noye_shared::ImportRowCounts {
                targets: t,
                channels: c,
                target_notifications: l,
                maintenance_windows: m,
                users: u,
                skipped: 0,
                replaced: 0,
            },
            warnings,
        };
        return Response::from_json(&result);
    }

    // Real import.
    let counts = db::migration::import_all(&d, &body.payload.data, body.on_conflict).await?;

    let detail = format!(
        "policy={:?} targets={} channels={} links={} maintenance={} users={} skipped={} replaced={}",
        body.on_conflict,
        counts.targets,
        counts.channels,
        counts.target_notifications,
        counts.maintenance_windows,
        counts.users,
        counts.skipped,
        counts.replaced,
    );
    let _ = db::audit::log(
        &d,
        &caller,
        "migration",
        "import",
        "import",
        None,
        Some(&detail),
    )
    .await;

    let result = ImportResult {
        applied: true,
        schema_version: MIGRATION_SCHEMA_VERSION,
        conflict_policy: body.on_conflict,
        rows: counts,
        warnings,
    };
    Response::from_json(&result)
}
