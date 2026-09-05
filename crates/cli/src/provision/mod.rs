//! `noye provision`: deployment in minutes, not hours (subject 37,
//! closes G-45/G-46).
//!
//! ## Design
//!
//! **Plan and apply are separate.** `--plan` resolves and prints every
//! action without changing anything; `--apply` executes it. Nothing
//! about `--plan` needs `wrangler` on `PATH`, network access, or
//! authentication -- it's pure logic, which is what makes it fully
//! testable under standing rule 7.
//!
//! **One [`Step`] trait, one list.** Idempotent re-run (repairing drift
//! on an existing deployment) is out of scope for this subject -- the
//! owner's condition on that scope was that adding it later stays a
//! structural addition, not a rewrite. The extension point is one added
//! method (`state() -> Satisfied | Missing | Drifted`) and a filter in
//! the runner; nothing above this module's [`run_plan`]/[`run_apply`]
//! signatures needs to change. See `tests.rs`'s `t235_*` test, which
//! asserts this compiles rather than merely documenting it.
//!
//! **Fail closed, report resumable state.** If a step fails partway
//! through `--apply`, [`resumable_state_report`] prints exactly which
//! resources now exist and which do not -- the operator never has to
//! guess what a half-run left behind, and a future idempotent re-run
//! has the same information available to it.

pub mod config;
pub mod ids;
pub mod names;
pub mod steps;

use anyhow::{Context, Result};
use names::Names;
use std::path::PathBuf;

/// Everything a [`Step`] reads or writes as `provision` runs. Resolved
/// fields (`d1_database_id`, `kv_namespace_id`, `shared_secret`) start
/// `None` and are filled in by the step responsible for producing them;
/// later steps that need one fail with a clear "ran out of order" error
/// rather than panicking on an unwrap, which is also what makes
/// deliberately testing an out-of-order runner meaningful (see
/// `t235_running_steps_out_of_order_fails_closed_not_panics`).
pub struct Plan {
    pub names: Names,
    pub force: bool,
    pub core_config_path: PathBuf,
    pub gateway_config_path: PathBuf,

    pub d1_database_id: Option<String>,
    pub kv_namespace_id: Option<String>,
    pub shared_secret: Option<String>,

    pub oidc_issuer_url: Option<String>,
    pub oidc_client_id: Option<String>,
    pub oidc_client_secret: Option<String>,
    pub admin_email: Option<String>,
    pub admin_name: Option<String>,

    /// Names of steps that completed successfully, in run order. What
    /// [`resumable_state_report`] reads.
    pub completed: Vec<&'static str>,
}

impl Plan {
    pub fn new(names: Names, force: bool) -> Self {
        let core_config_path = PathBuf::from("crates/core/wrangler.toml");
        let gateway_config_path = PathBuf::from("crates/gateway/wrangler.toml");
        Plan {
            names,
            force,
            core_config_path,
            gateway_config_path,
            d1_database_id: None,
            kv_namespace_id: None,
            shared_secret: None,
            oidc_issuer_url: None,
            oidc_client_id: None,
            oidc_client_secret: None,
            admin_email: None,
            admin_name: None,
            completed: Vec::new(),
        }
    }
}

/// One provisioning action. `describe` must never read anything a human
/// shouldn't see printed (see design note above and `t230_*` tests) --
/// it is called for every step during `--plan`, before anything has
/// been generated or created.
pub trait Step {
    /// Stable identifier, used for resumable-state reporting. Not
    /// shown to the operator directly except inside that report.
    fn name(&self) -> &'static str;
    /// Human-readable description of what this step will do, resolved
    /// against `plan`'s derived names. Called for every step under
    /// `--plan`, and must not spawn a subprocess or touch the
    /// filesystem.
    fn describe(&self, plan: &Plan) -> String;
    /// Perform the action, updating `plan` with anything later steps
    /// depend on (a resolved id, the generated secret).
    fn apply(&self, plan: &mut Plan) -> Result<()>;
}

/// `--plan`: print every step's description. Never calls `apply` on
/// anything -- see `t229_*` tests, which use steps whose `apply` would
/// fail the test if it were ever reached.
pub fn run_plan(steps: &[Box<dyn Step>], plan: &Plan) {
    for step in steps {
        println!("{}", step.describe(plan));
    }
}

/// `--apply`: run every step in order. On the first failure, print the
/// resumable-state report and stop -- steps after the failed one are
/// never attempted.
pub fn run_apply(steps: &[Box<dyn Step>], plan: &mut Plan) -> Result<()> {
    for step in steps {
        let name = step.name();
        step.apply(plan).with_context(|| {
            let report = resumable_state_report(steps, plan);
            format!("step '{name}' failed\n\n{report}")
        })?;
        plan.completed.push(name);
    }
    Ok(())
}

/// What a human needs after a partial `--apply`: which resources now
/// exist, which do not. T-234.
pub fn resumable_state_report(steps: &[Box<dyn Step>], plan: &Plan) -> String {
    let mut out = String::from("Resources that now exist:\n");
    if plan.completed.is_empty() {
        out.push_str("  (none)\n");
    }
    for name in &plan.completed {
        out.push_str(&format!("  - {name}\n"));
    }
    out.push_str("Resources that do NOT exist yet:\n");
    for step in steps {
        if !plan.completed.contains(&step.name()) {
            out.push_str(&format!("  - {}\n", step.name()));
        }
    }
    out
}

/// A 32-byte secret, hex-encoded -- same shape as `setup.md`'s current
/// `openssl rand -hex 32`. Deliberately returns an owned `String` with
/// no `Display`/`Debug` wrapper that could be accidentally logged; the
/// caller is responsible for the same discipline `docs/src/setup.md`'s
/// replacement text now documents (never printed, piped straight to
/// `wrangler secret put`).
pub fn generate_shared_secret() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The three things `provision` cannot do, printed before anything
/// else runs so nobody discovers them halfway (subject 37, "What stays
/// human").
pub fn human_prerequisites_banner() -> String {
    "Before this command can do anything, you still need to:\n\
     \n\
     1. `wrangler login`\n\
     2. Register an OAuth client at your identity provider\n\
     3. Have ready: issuer URL, client id, client secret, admin email\n"
        .to_string()
}

#[cfg(test)]
mod tests;
