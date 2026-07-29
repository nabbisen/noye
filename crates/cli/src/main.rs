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
use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::Command;

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

// SCRATCH TEST ONLY — T-170 (rfcs/handoffs/03c-ci-toolchain-install.md).
// Deliberate clippy::bool_comparison + rustfmt violation, proving the
// gate fails on a real lint/format problem rather than merely running
// with nothing to catch. This branch is discarded after the CI run
// confirms failure — never merge this function.
fn t170_scratch_violation(x: bool) -> bool {
    if x==true { return true; } else { return false; }
}

fn main() -> Result<()> {
    let _ = t170_scratch_violation(true);
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
    }
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
