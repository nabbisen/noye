//! Resource and worker name derivation from `--env-name` (G-46).
//!
//! One input, four outputs, so a staging deployment on the same
//! Cloudflare account never collides with production. Binding names
//! (`DB`, `CACHE_KV`, `LOG_BUCKET`, `CORE`) are deliberately **not**
//! derived here -- they are per-worker identifiers Rust code refers to
//! by name, not account-scoped resources, and changing them would break
//! code for no benefit.

/// The default environment name. Deriving names for this one reproduces
/// the literal defaults every `wrangler.toml.example` already ships --
/// existing single-deployment setups see no change.
pub const DEFAULT_ENV_NAME: &str = "noye";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Names {
    pub env_name: String,
    pub core_worker: String,
    pub gateway_worker: String,
    pub d1_database: String,
    pub r2_bucket: String,
}

impl Names {
    /// Derive every resource/worker name from a single `env_name`.
    ///
    /// `env_name == "noye"` (the default) reproduces the literal names
    /// already in both `wrangler.toml.example` files exactly: no
    /// `noye-noye-core` doubling. Any other `env_name` inserts itself
    /// between the fixed `noye` prefix and the resource suffix, with a
    /// `-` for worker/bucket names and `_` for the D1 database (D1
    /// database names disallow `-` in this project's existing
    /// convention: `noye_db`, not `noye-db`).
    pub fn derive(env_name: &str) -> Self {
        let dash = if env_name == DEFAULT_ENV_NAME {
            String::new()
        } else {
            format!("-{env_name}")
        };
        let underscore = if env_name == DEFAULT_ENV_NAME {
            String::new()
        } else {
            format!("_{env_name}")
        };
        Names {
            env_name: env_name.to_string(),
            core_worker: format!("noye{dash}-core"),
            gateway_worker: format!("noye{dash}-gateway"),
            d1_database: format!("noye{underscore}_db"),
            r2_bucket: format!("noye{dash}-logs"),
        }
    }
}

#[cfg(test)]
mod tests;
