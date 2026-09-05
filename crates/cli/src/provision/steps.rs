//! Concrete provisioning steps. Each wraps `wrangler` as a subprocess,
//! the same choice `crates/cli/src/main.rs`'s `admin create` already
//! made and documented ("Why a binary crate, not a shell script") --
//! wrangler already owns authentication, profile selection, and the
//! local/remote split.
//!
//! `apply()` bodies here are integration-shaped (they spawn `wrangler`,
//! sometimes against a real account) and are **not** unit tested,
//! matching this crate's existing precedent for `run_wrangler_d1`: only
//! `describe()` (pure string formatting) is. `--apply` against a real
//! account is the owner's to run, during the deployment session --
//! standing rule 7.

use super::{Plan, Step};
use anyhow::{Context, Result, anyhow};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const CORE_DIR: &str = "crates/core";
const GATEWAY_DIR: &str = "crates/gateway";

fn run_wrangler_capture(dir: &str, args: &[&str]) -> Result<String> {
    let output = Command::new("wrangler")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("failed to invoke `wrangler {}` in {dir}", args.join(" ")))?;
    if !output.status.success() {
        return Err(anyhow!(
            "wrangler {} exited with status {:?}: {}",
            args.join(" "),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn run_wrangler_status(dir: &str, args: &[&str]) -> Result<()> {
    let status = Command::new("wrangler")
        .args(args)
        .current_dir(dir)
        .status()
        .with_context(|| format!("failed to invoke `wrangler {}` in {dir}", args.join(" ")))?;
    if !status.success() {
        return Err(anyhow!(
            "wrangler {} exited with status {:?} -- see output above",
            args.join(" "),
            status.code()
        ));
    }
    Ok(())
}

/// Pipe `value` on stdin rather than passing it as an argument -- an
/// argument lands in the process list and shell history; stdin does
/// not. `wrangler secret put` reads this way by design.
fn run_wrangler_stdin(dir: &str, args: &[&str], value: &str) -> Result<()> {
    let mut child = Command::new("wrangler")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to invoke `wrangler {}` in {dir}", args.join(" ")))?;
    child
        .stdin
        .as_mut()
        .expect("stdin was requested as piped")
        .write_all(value.as_bytes())
        .context("failed to write to wrangler's stdin")?;
    let status = child.wait().context("failed to wait on wrangler")?;
    if !status.success() {
        return Err(anyhow!(
            "wrangler {} exited with status {:?}",
            args.join(" "),
            status.code()
        ));
    }
    Ok(())
}

/// The complete, ordered step list. Order is the enforcement mechanism
/// for design point 9 (Core before Gateway): nothing later in this
/// list can run before something earlier in it does.
pub fn default_steps() -> Vec<Box<dyn Step>> {
    vec![
        Box::new(CreateD1),
        Box::new(ResolveD1Id),
        Box::new(CreateKv),
        Box::new(ResolveKvId),
        Box::new(CreateR2),
        Box::new(WriteCoreConfig),
        Box::new(WriteGatewayConfig),
        Box::new(ApplyMigrations),
        Box::new(GenerateAndRegisterSharedSecret),
        Box::new(RegisterOidcClientSecret),
        Box::new(CreateAdminUser),
        Box::new(DeployCore),
        Box::new(DeployGateway),
    ]
}

pub struct CreateD1;
impl Step for CreateD1 {
    fn name(&self) -> &'static str {
        "create_d1"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!("Create D1 database '{}'", plan.names.d1_database)
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        run_wrangler_capture(CORE_DIR, &["d1", "create", &plan.names.d1_database])?;
        Ok(())
    }
}

pub struct ResolveD1Id;
impl Step for ResolveD1Id {
    fn name(&self) -> &'static str {
        "resolve_d1_id"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Resolve the database id for '{}' (wrangler d1 info --json)",
            plan.names.d1_database
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let stdout =
            run_wrangler_capture(CORE_DIR, &["d1", "info", &plan.names.d1_database, "--json"])?;
        let id = super::ids::parse_d1_database_id(&stdout).map_err(|e| {
            anyhow!(
                "could not resolve the D1 database id from `wrangler d1 info --json` ({e}) -- \
                 wrangler's output shape may have changed; report this rather than guessing"
            )
        })?;
        plan.d1_database_id = Some(id);
        Ok(())
    }
}

pub struct CreateKv;
impl Step for CreateKv {
    fn name(&self) -> &'static str {
        "create_kv"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Create KV namespace for '{}' (session store + OIDC state + JWKS cache)",
            plan.names.gateway_worker
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let namespace = format!("{}-cache", plan.names.gateway_worker);
        run_wrangler_capture(GATEWAY_DIR, &["kv", "namespace", "create", &namespace])?;
        Ok(())
    }
}

pub struct ResolveKvId;
impl Step for ResolveKvId {
    fn name(&self) -> &'static str {
        "resolve_kv_id"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Resolve the KV namespace id for '{}-cache' (no machine-readable wrangler output exists for this -- you will be prompted to paste it)",
            plan.names.gateway_worker
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let namespace = format!("{}-cache", plan.names.gateway_worker);
        let id = super::ids::prompt_for_kv_namespace_id_interactive(&namespace)?;
        plan.kv_namespace_id = Some(id);
        Ok(())
    }
}

pub struct CreateR2;
impl Step for CreateR2 {
    fn name(&self) -> &'static str {
        "create_r2"
    }
    fn describe(&self, plan: &Plan) -> String {
        // R2 is referenced by name only -- no id to resolve, per
        // design point 5.
        format!("Create R2 bucket '{}'", plan.names.r2_bucket)
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        run_wrangler_capture(CORE_DIR, &["r2", "bucket", "create", &plan.names.r2_bucket])?;
        Ok(())
    }
}

fn require<'a>(value: &'a Option<String>, what: &str) -> Result<&'a str> {
    value
        .as_deref()
        .ok_or_else(|| anyhow!("internal ordering error: {what} was needed before it was resolved"))
}

/// Pure: render Core's config from its template. Separated from
/// [`WriteCoreConfig::apply`] so the real committed `.example` file can
/// be exercised in a test without touching the filesystem or `--force`
/// semantics -- see `steps/tests.rs`'s
/// `render_core_config_against_the_real_template_produces_valid_toml`.
pub fn render_core_config(template: &str, plan: &Plan, database_id: &str) -> String {
    super::config::render(
        template,
        &[
            (
                r#"name = "noye-core""#,
                &format!(r#"name = "{}""#, plan.names.core_worker),
            ),
            (
                r#"database_name = "noye_db""#,
                &format!(r#"database_name = "{}""#, plan.names.d1_database),
            ),
            (
                r#"database_id = "YOUR_DATABASE_ID_HERE""#,
                &format!(r#"database_id = "{database_id}""#),
            ),
            (
                r#"bucket_name = "noye-logs""#,
                &format!(r#"bucket_name = "{}""#, plan.names.r2_bucket),
            ),
        ],
    )
}

pub struct WriteCoreConfig;
impl Step for WriteCoreConfig {
    fn name(&self) -> &'static str {
        "write_core_config"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Write {} from crates/core/wrangler.toml.example (worker '{}', database '{}', bucket '{}')",
            plan.core_config_path.display(),
            plan.names.core_worker,
            plan.names.d1_database,
            plan.names.r2_bucket,
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let database_id = require(&plan.d1_database_id, "the D1 database id")?.to_string();
        let template_path = Path::new("crates/core/wrangler.toml.example");
        let template = std::fs::read_to_string(template_path)
            .with_context(|| format!("failed to read {}", template_path.display()))?;
        let contents = render_core_config(&template, plan, &database_id);
        super::config::write_config_atomic(&plan.core_config_path, &contents, plan.force)
    }
}

/// Pure: render Gateway's config from its template. See
/// [`render_core_config`]'s doc for why this is separated from `apply`.
pub fn render_gateway_config(template: &str, plan: &Plan, kv_id: &str) -> String {
    let mut contents = super::config::render(
        template,
        &[
            (
                r#"name = "noye-gateway""#,
                &format!(r#"name = "{}""#, plan.names.gateway_worker),
            ),
            (r#"id = "YOUR_KV_ID_HERE""#, &format!(r#"id = "{kv_id}""#)),
            (
                r#"service = "noye-core""#,
                &format!(r#"service = "{}""#, plan.names.core_worker),
            ),
        ],
    );
    if let Some(issuer) = &plan.oidc_issuer_url {
        contents = contents.replace(
            r#"OIDC_ISSUER_URL = "http://localhost:5556""#,
            &format!(r#"OIDC_ISSUER_URL = "{issuer}""#),
        );
    }
    if let Some(client_id) = &plan.oidc_client_id {
        contents = contents.replace(
            r#"OIDC_CLIENT_ID = "noye-local-client""#,
            &format!(r#"OIDC_CLIENT_ID = "{client_id}""#),
        );
    }
    contents
}

pub struct WriteGatewayConfig;
impl Step for WriteGatewayConfig {
    fn name(&self) -> &'static str {
        "write_gateway_config"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Write {} from crates/gateway/wrangler.toml.example (worker '{}', service binding to '{}')",
            plan.gateway_config_path.display(),
            plan.names.gateway_worker,
            plan.names.core_worker,
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let kv_id = require(&plan.kv_namespace_id, "the KV namespace id")?.to_string();
        let template_path = Path::new("crates/gateway/wrangler.toml.example");
        let template = std::fs::read_to_string(template_path)
            .with_context(|| format!("failed to read {}", template_path.display()))?;
        let contents = render_gateway_config(&template, plan, &kv_id);
        super::config::write_config_atomic(&plan.gateway_config_path, &contents, plan.force)
    }
}

pub struct ApplyMigrations;
impl Step for ApplyMigrations {
    fn name(&self) -> &'static str {
        "apply_migrations"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Apply sql/ migrations to '{}' (wrangler d1 migrations apply)",
            plan.names.d1_database
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        run_wrangler_status(
            CORE_DIR,
            &[
                "d1",
                "migrations",
                "apply",
                &plan.names.d1_database,
                "--remote",
            ],
        )
    }
}

pub struct GenerateAndRegisterSharedSecret;
impl Step for GenerateAndRegisterSharedSecret {
    fn name(&self) -> &'static str {
        "shared_secret"
    }
    fn describe(&self, _plan: &Plan) -> String {
        // Never interpolates a value -- there is nothing to generate
        // yet at describe() time, and there never will be anything to
        // print even after apply() runs. See t230_* tests.
        "Generate a shared secret and register it as GATEWAY_SHARED_TOKEN on both workers \
         (never printed, piped directly to `wrangler secret put`)"
            .to_string()
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let secret = super::generate_shared_secret();
        run_wrangler_stdin(
            GATEWAY_DIR,
            &["secret", "put", "GATEWAY_SHARED_TOKEN"],
            &secret,
        )?;
        run_wrangler_stdin(
            CORE_DIR,
            &["secret", "put", "GATEWAY_SHARED_TOKEN"],
            &secret,
        )?;
        plan.shared_secret = Some(secret);
        Ok(())
    }
}

pub struct RegisterOidcClientSecret;
impl Step for RegisterOidcClientSecret {
    fn name(&self) -> &'static str {
        "oidc_client_secret"
    }
    fn describe(&self, _plan: &Plan) -> String {
        "Register the OIDC client secret as OIDC_CLIENT_SECRET on the Gateway (never printed)"
            .to_string()
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let secret = require(&plan.oidc_client_secret, "the OIDC client secret")?.to_string();
        run_wrangler_stdin(
            GATEWAY_DIR,
            &["secret", "put", "OIDC_CLIENT_SECRET"],
            &secret,
        )
    }
}

pub struct CreateAdminUser;
impl Step for CreateAdminUser {
    fn name(&self) -> &'static str {
        "create_admin"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!(
            "Create the initial admin user via `noye admin create` (email: {})",
            plan.admin_email.as_deref().unwrap_or("<not yet supplied>")
        )
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        let email = require(&plan.admin_email, "the admin email")?.to_string();
        let name = require(&plan.admin_name, "the admin name")?.to_string();
        // Reuse `admin create` rather than re-implementing the INSERT
        // (subject 37, design point 7) -- shell back into this same
        // binary rather than duplicating admin_create's SQL. No shell
        // is involved (`Command::args` passes argv elements directly),
        // and `admin create` already validates/escapes internally
        // (`validate_email`, `sql_escape` in `main.rs`).
        let exe = std::env::current_exe().context("could not determine current executable path")?;
        let status = Command::new(exe)
            .args([
                "admin", "create", "--email", &email, "--name", &name, "--remote",
            ])
            .status()
            .context("failed to invoke `noye admin create`")?;
        if !status.success() {
            return Err(anyhow!(
                "`noye admin create` exited with status {:?}",
                status.code()
            ));
        }
        Ok(())
    }
}

pub struct DeployCore;
impl Step for DeployCore {
    fn name(&self) -> &'static str {
        "deploy_core"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!("Deploy Core ('{}')", plan.names.core_worker)
    }
    fn apply(&self, _plan: &mut Plan) -> Result<()> {
        run_wrangler_status(CORE_DIR, &["deploy"])
    }
}

pub struct DeployGateway;
impl Step for DeployGateway {
    fn name(&self) -> &'static str {
        "deploy_gateway"
    }
    fn describe(&self, plan: &Plan) -> String {
        format!("Deploy Gateway ('{}')", plan.names.gateway_worker)
    }
    fn apply(&self, plan: &mut Plan) -> Result<()> {
        if !plan.completed.contains(&"deploy_core") {
            return Err(anyhow!(
                "refusing to deploy Gateway before Core -- the Service Binding requires Core \
                 to exist first (design point 9)"
            ));
        }
        run_wrangler_status(GATEWAY_DIR, &["deploy"])
    }
}

#[cfg(test)]
mod tests;
