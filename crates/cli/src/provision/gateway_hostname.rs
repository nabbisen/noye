//! Resolving `--gateway-hostname` into the one fact that decides both
//! reachability and the OIDC redirect URI (subject 37a, closes G-48).
//!
//! `noye provision --apply` used to complete successfully and produce
//! a deployment nobody could reach or log into: the Gateway's
//! `wrangler.toml.example` ships `workers_dev = false` with no route,
//! and `OIDC_REDIRECT_URI` was never rendered, staying at the
//! template's `http://localhost:8787/auth/callback`. `--gateway-
//! hostname <host>` is deliberately the *only* input for both facts --
//! two inputs for one fact is the drift subject 37 already avoided for
//! the Service Binding (Gateway's `service = "..."` always follows
//! Core's own derived name rather than a separately-carried value).

use std::fmt;

/// How the Gateway becomes reachable, derived entirely from the
/// resolved hostname's shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachability {
    /// `<host>` ends in `.workers.dev`: Cloudflare's own subdomain,
    /// enabled by `workers_dev = true`. No `[[routes]]` entry needed
    /// or written.
    WorkersDev,
    /// Anything else: `workers_dev = false` (unchanged from the
    /// template's own default) plus a `[[routes]]` entry binding the
    /// custom domain to this worker.
    CustomDomain,
}

/// The resolved hostname and everything it determines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GatewayHostname {
    pub host: String,
    pub reachability: Reachability,
    /// Always `https://<host>/auth/callback` -- the one thing that
    /// must never be computed a second, independent way.
    pub redirect_uri: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum HostnameError {
    /// `--gateway-hostname` was not supplied at all.
    Missing,
    /// A `.workers.dev` host whose first label doesn't match the
    /// derived Gateway worker name -- a copy-paste error that would
    /// deploy to one worker and point OIDC's redirect at another.
    WorkersDevLabelMismatch {
        host: String,
        expected_label: String,
        found_label: String,
    },
}

impl fmt::Display for HostnameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HostnameError::Missing => write!(
                f,
                "--gateway-hostname is required: without it, `provision` would produce a \
                 deployment nobody can reach or log into. Supply either a Cloudflare \
                 *.workers.dev host (e.g. <worker-name>.<account>.workers.dev) or a custom \
                 domain you have already pointed at Cloudflare (e.g. noye.example.com)"
            ),
            HostnameError::WorkersDevLabelMismatch {
                host,
                expected_label,
                found_label,
            } => write!(
                f,
                "--gateway-hostname '{host}' is a workers.dev host whose first label \
                 ('{found_label}') does not match the derived Gateway worker name \
                 ('{expected_label}') -- this would deploy to '{expected_label}' and point \
                 OIDC's redirect at '{found_label}' instead, a copy-paste error, not a valid \
                 deployment"
            ),
        }
    }
}

/// Resolve `--gateway-hostname` against the derived Gateway worker
/// name. `host` being `None` (the flag omitted) and a `.workers.dev`
/// host whose first label disagrees with `gateway_worker` are the only
/// two ways this fails -- both refusals, never a guess.
pub fn resolve(host: Option<&str>, gateway_worker: &str) -> Result<GatewayHostname, HostnameError> {
    let host = host.ok_or(HostnameError::Missing)?;

    if let Some(first_label) = host.strip_suffix(".workers.dev").map(|rest| {
        // strip_suffix leaves "<label>.<account>"; the first label is
        // everything before the first remaining '.'.
        rest.split('.').next().unwrap_or("")
    }) {
        if first_label != gateway_worker {
            return Err(HostnameError::WorkersDevLabelMismatch {
                host: host.to_string(),
                expected_label: gateway_worker.to_string(),
                found_label: first_label.to_string(),
            });
        }
        return Ok(GatewayHostname {
            host: host.to_string(),
            reachability: Reachability::WorkersDev,
            redirect_uri: format!("https://{host}/auth/callback"),
        });
    }

    Ok(GatewayHostname {
        host: host.to_string(),
        reachability: Reachability::CustomDomain,
        redirect_uri: format!("https://{host}/auth/callback"),
    })
}

#[cfg(test)]
mod tests;
