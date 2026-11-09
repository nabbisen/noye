//! Noye CLI: operational tasks against a Noye deployment.
//!
//! Today this is a thin wrapper around `wrangler d1 execute` that shapes
//! SQL for the most common one-time setup chore: creating an initial admin
//! user. Wrapping `wrangler` (rather than talking to the Cloudflare API
//! directly) means we inherit `wrangler`'s authentication, profile, and
//! environment selection without re-implementing any of it.
//!
//! Designed with future tenancy in mind: subcommands are grouped under
//! resource names (`admin`, `user`, …) so adding `tenant create` later
//! is a structural addition rather than a refactor.
//!
//! ## Why a binary crate, not a shell script
//!
//! - clap gives a real argument parser with `--help`, value validation,
//!   and shell completions.
//! - We can reuse the workspace types (e.g. share user-row construction
//!   with the migration validator if that becomes useful later).
//! - Errors are typed (`anyhow::Result`) so we can surface what failed
//!   instead of a SQL parser error from wrangler.

use anyhow::{Context, Result, anyhow};
use clap::{ArgGroup, Parser, Subcommand};
use std::io::BufRead;
use std::path::PathBuf;
use std::process::Command;

mod provision;

#[derive(Parser, Debug)]
#[command(
    name = "noye",
    version,
    about = "Operational tasks for a Noye deployment"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Initial admin and user-management commands
    Admin {
        #[command(subcommand)]
        action: AdminAction,
    },
    /// Read-only inspection of the users table
    User {
        #[command(subcommand)]
        action: UserAction,
    },
    /// Provision a fresh Noye deployment: D1/KV/R2, migrations, secrets,
    /// the initial admin user, and both Workers -- in one command
    /// instead of `docs/src/setup.md`'s ten-step walkthrough.
    ///
    /// First-run only: this does not repair or reconcile an existing
    /// deployment (see `crates/cli/src/provision/mod.rs`'s module doc
    /// for the extension point that adds that later).
    Provision(ProvisionArgs),
}

#[derive(clap::Args, Debug)]
#[command(group(ArgGroup::new("mode").args(["plan", "apply"]).required(true)))]
struct ProvisionArgs {
    /// Print every action, resolved against --env-name, without
    /// changing anything. Spawns no subprocess and writes no file.
    #[arg(long)]
    plan: bool,
    /// Execute the plan against a real Cloudflare account.
    #[arg(long)]
    apply: bool,
    /// Prefix for every derived resource and worker name. The default
    /// reproduces the literal names already in both
    /// `wrangler.toml.example` files exactly.
    #[arg(long, default_value = "noye")]
    env_name: String,
    /// Overwrite an existing `wrangler.toml` instead of refusing.
    #[arg(long, default_value_t = false)]
    force: bool,
    /// The host the Gateway will be reachable at: either a Cloudflare
    /// *.workers.dev host (first label must equal the derived Gateway
    /// worker name) or a custom domain already pointed at Cloudflare.
    /// Required -- determines both `workers_dev`/`[[routes]]` and
    /// `OIDC_REDIRECT_URI` together, so they cannot drift apart (G-48).
    #[arg(long)]
    gateway_hostname: Option<String>,
    /// OIDC issuer URL. Prompted interactively under --apply if omitted.
    #[arg(long)]
    oidc_issuer_url: Option<String>,
    /// OIDC client id. Prompted interactively under --apply if omitted.
    #[arg(long)]
    oidc_client_id: Option<String>,
    /// OIDC client secret. Prompted interactively under --apply if
    /// omitted -- never pass this on the command line in a shared
    /// shell; prefer the prompt.
    #[arg(long)]
    oidc_client_secret: Option<String>,
    /// Initial admin's email (must match the OIDC provider's email
    /// claim). Prompted interactively under --apply if omitted.
    #[arg(long)]
    admin_email: Option<String>,
    /// Initial admin's display name. Prompted interactively under
    /// --apply if omitted.
    #[arg(long)]
    admin_name: Option<String>,
}

#[derive(Subcommand, Debug)]
enum AdminAction {
    /// Insert a new admin user into the Noye `users` table.
    ///
    /// Idempotent: re-running with the same email produces the same row
    /// (INSERT OR IGNORE). Use `noye user list` to confirm.
    Create {
        /// Email address of the admin (must match the OIDC IdP claim)
        #[arg(long)]
        email: String,
        /// Display name shown in the UI
        #[arg(long)]
        name: String,
        /// Run against the remote D1 database. Default is the local
        /// Miniflare-managed database used by `wrangler dev`.
        #[arg(long, default_value_t = false)]
        remote: bool,
        /// Override the default D1 binding name (`noye_db`).
        #[arg(long, default_value = "noye_db")]
        database: String,
        /// Override the path to the wrangler config (defaults to
        /// `crates/core/wrangler.toml` when run from the workspace root).
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

#[derive(Subcommand, Debug)]
enum UserAction {
    /// List all users in the deployment.
    List {
        #[arg(long, default_value_t = false)]
        remote: bool,
        #[arg(long, default_value = "noye_db")]
        database: String,
        #[arg(long)]
        config: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Admin { action } => match action {
            AdminAction::Create {
                email,
                name,
                remote,
                database,
                config,
            } => admin_create(&email, &name, remote, &database, config.as_deref()),
        },
        Commands::User { action } => match action {
            UserAction::List {
                remote,
                database,
                config,
            } => user_list(remote, &database, config.as_deref()),
        },
        Commands::Provision(args) => provision_run(args),
    }
}

fn provision_run(args: ProvisionArgs) -> Result<()> {
    let names = provision::names::Names::derive(&args.env_name);

    // Resolved before anything else -- a `Plan`, the step list, even
    // `--plan`'s own printing -- so a missing or mismatched
    // --gateway-hostname fails before any step could possibly run
    // (subject 37a, G-48, T-240). Defaulting silently to either shape
    // is how the Gateway ended up unreachable in the first place.
    let gateway_hostname = provision::gateway_hostname::resolve(
        args.gateway_hostname.as_deref(),
        &names.gateway_worker,
    )
    .map_err(|e| anyhow!("{e}"))?;

    let mut plan = provision::Plan::new(names, args.force);
    plan.gateway_hostname = Some(gateway_hostname);
    plan.oidc_issuer_url = args.oidc_issuer_url;
    plan.oidc_client_id = args.oidc_client_id;
    plan.oidc_client_secret = args.oidc_client_secret;
    plan.admin_email = args.admin_email;
    plan.admin_name = args.admin_name;

    let steps = provision::steps::default_steps();

    if args.plan {
        println!("{}", provision::human_prerequisites_banner());
        provision::run_plan(&steps, &plan);
        return Ok(());
    }

    // --apply from here on.
    println!("{}", provision::human_prerequisites_banner());

    // Solve setup.md's ordering problem: the operator needs the
    // Gateway's redirect URI before they can finish registering the
    // OAuth client, but the URL only exists once Gateway is deployed.
    // The hostname is resolved now, above -- print the *real* redirect
    // URI, not a placeholder, before asking for OIDC values (subject
    // 37, "Solve the ordering problem while you are there").
    println!(
        "Gateway's redirect URI will be:\n  {}\nRegister that with your OIDC provider before continuing.\n",
        plan.gateway_hostname
            .as_ref()
            .expect("resolved above")
            .redirect_uri
    );

    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    if plan.oidc_issuer_url.is_none() {
        plan.oidc_issuer_url = Some(prompt("OIDC issuer URL", &mut stdin)?);
    }
    if plan.oidc_client_id.is_none() {
        plan.oidc_client_id = Some(prompt("OIDC client id", &mut stdin)?);
    }
    if plan.oidc_client_secret.is_none() {
        plan.oidc_client_secret = Some(prompt("OIDC client secret", &mut stdin)?);
    }
    if plan.admin_email.is_none() {
        plan.admin_email = Some(prompt("Initial admin email", &mut stdin)?);
    }
    if plan.admin_name.is_none() {
        plan.admin_name = Some(prompt("Initial admin display name", &mut stdin)?);
    }

    provision::run_apply(&steps, &mut plan)
}

fn prompt(label: &str, reader: &mut impl BufRead) -> Result<String> {
    use std::io::Write;
    print!("{label}: ");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    reader
        .read_line(&mut line)
        .with_context(|| format!("failed to read {label} from stdin"))?;
    Ok(line.trim().to_string())
}

fn admin_create(
    email: &str,
    name: &str,
    remote: bool,
    database: &str,
    config: Option<&std::path::Path>,
) -> Result<()> {
    validate_email(email)?;

    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // INSERT OR IGNORE keeps the command idempotent. If the email is
    // already present (matched on the unique constraint), the row stays
    // as it was — the operator can detect "already exists" via the
    // following `user list` call.
    let sql = format!(
        "INSERT OR IGNORE INTO users \
         (id, email, name, role, is_active, created_at, updated_at) \
         VALUES ('{id}', '{email}', '{name}', 'admin', 1, '{now}', '{now}')",
        id = sql_escape(&id),
        email = sql_escape(email),
        name = sql_escape(name),
        now = sql_escape(&now),
    );

    let target = if remote { "remote" } else { "local" };
    println!(
        "Creating admin user against {} database '{}'...",
        target, database
    );

    run_wrangler_d1(&sql, remote, database, config)?;

    println!(
        "Done. Verify with: noye user list{}",
        if remote { " --remote" } else { "" }
    );
    Ok(())
}

fn user_list(remote: bool, database: &str, config: Option<&std::path::Path>) -> Result<()> {
    let sql = "SELECT id, email, name, role, is_active, created_at FROM users ORDER BY created_at";
    run_wrangler_d1(sql, remote, database, config)?;
    Ok(())
}

/// Run a SQL statement via `wrangler d1 execute`.
///
/// We call wrangler as a subprocess rather than re-implementing D1's HTTP
/// API: wrangler already handles authentication, profile selection, the
/// local-vs-remote split, and the migrations layout.
fn run_wrangler_d1(
    sql: &str,
    remote: bool,
    database: &str,
    config: Option<&std::path::Path>,
) -> Result<()> {
    let mut cmd = Command::new("wrangler");
    cmd.arg("d1").arg("execute").arg(database);

    if remote {
        cmd.arg("--remote");
    } else {
        cmd.arg("--local");
    }

    if let Some(cfg) = config {
        cmd.arg("--config").arg(cfg);
    } else {
        // The wrangler config that owns the D1 binding is on the Core.
        // We default to it so this works straight from the workspace root.
        let default = std::path::Path::new("crates/core/wrangler.toml");
        if default.exists() {
            cmd.arg("--config").arg(default);
        }
        // If neither was supplied and the default doesn't exist, let
        // wrangler look in CWD as it normally would. This keeps the CLI
        // usable from the `crates/core/` directory directly.
    }

    cmd.arg("--command").arg(sql);

    let status = cmd.status().with_context(
        || "failed to invoke `wrangler` (is it installed and on PATH? `npm install -g wrangler`)",
    )?;

    if !status.success() {
        return Err(anyhow!(
            "wrangler exited with status {} — see output above",
            status
                .code()
                .map(|c| c.to_string())
                .unwrap_or_else(|| "(killed)".to_string())
        ));
    }
    Ok(())
}

/// Reject shell-injection-prone or obviously malformed email values.
///
/// We pass the email through `--command "..."` which is a single argv
/// slot, so shell injection is not actually possible — but a bad email
/// would silently produce a row that no OIDC IdP claim will ever match,
/// which is worse. Reject early so the operator notices.
fn validate_email(email: &str) -> Result<()> {
    if email.is_empty() {
        return Err(anyhow!("email must not be empty"));
    }
    if !email.contains('@') {
        return Err(anyhow!("email '{}' is missing '@'", email));
    }
    if email.len() > 254 {
        // RFC 5321 §4.5.3.1.3
        return Err(anyhow!("email exceeds 254 bytes (RFC 5321 limit)"));
    }
    if email.contains('\'') || email.contains('"') || email.contains(';') {
        return Err(anyhow!(
            "email '{}' contains characters we refuse to embed in SQL",
            email
        ));
    }
    Ok(())
}

/// Escape a single quote for safe interpolation into a SQL string literal.
///
/// We've already vetted the inputs in `validate_email` and uuid, but
/// `name` is operator-supplied free text and might have a legitimate `'`.
/// SQLite's escape rule is to double the quote.
fn sql_escape(s: &str) -> String {
    s.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_validation_accepts_canonical() {
        assert!(validate_email("alice@example.com").is_ok());
        assert!(validate_email("a.b+c@sub.example.org").is_ok());
    }

    #[test]
    fn email_validation_rejects_empty() {
        assert!(validate_email("").is_err());
    }

    #[test]
    fn email_validation_rejects_no_at_sign() {
        assert!(validate_email("alice").is_err());
    }

    #[test]
    fn email_validation_rejects_oversized() {
        let huge = format!("{}@example.com", "a".repeat(300));
        assert!(validate_email(&huge).is_err());
    }

    #[test]
    fn email_validation_rejects_dangerous_chars() {
        assert!(validate_email("alice'or'1=1@example.com").is_err());
        assert!(validate_email("alice;DROP@example.com").is_err());
        assert!(validate_email("alice\"@example.com").is_err());
    }

    #[test]
    fn sql_escape_doubles_single_quote() {
        assert_eq!(sql_escape("O'Brien"), "O''Brien");
    }

    #[test]
    fn sql_escape_passes_through_safe_strings() {
        assert_eq!(sql_escape("Alice Smith"), "Alice Smith");
        assert_eq!(sql_escape(""), "");
    }
}
