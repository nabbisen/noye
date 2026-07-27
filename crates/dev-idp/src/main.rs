//! Local development OIDC stub for Noye.
//!
//! Implements the minimum subset of OIDC Core 1.0 that the Noye gateway
//! needs to complete a login round-trip:
//!
//! - `GET /.well-known/openid-configuration` — Discovery document
//! - `GET /jwks` — JSON Web Key Set (the public side of our signing key)
//! - `GET /authorize` — Authorization Endpoint; stashes nonce/state and
//!   redirects to the gateway's callback with a code
//! - `POST /token` — Token Endpoint; consumes the code and returns a
//!   signed RS256 ID Token
//!
//! ## What this is NOT
//!
//! - Production-grade. Keys regenerate on every start, the user database
//!   is hard-coded, and there is no PKCE verifier strictness, no replay
//!   prevention beyond single-use codes, no client_secret check.
//! - Multi-user. A single account (`admin@local.test`) is served. To
//!   simulate other identities, edit `DEFAULT_USER` below or set the
//!   `DEV_IDP_USER_EMAIL` / `DEV_IDP_USER_NAME` env vars.
//!
//! ## Usage
//!
//! ```bash
//! cargo run -p noye-dev-idp
//! # listens on http://localhost:5556
//! ```
//!
//! Then in `crates/gateway/wrangler.toml`:
//!
//! ```toml
//! OIDC_ISSUER_URL = "http://localhost:5556"
//! OIDC_CLIENT_ID  = "noye-local-client"
//! OIDC_REDIRECT_URI = "http://localhost:8787/auth/callback"
//! ```
//!
//! and a Gateway secret of any non-empty value:
//!
//! ```bash
//! echo "any-non-empty-string" | wrangler secret put OIDC_CLIENT_SECRET
//! ```

mod handlers;
mod jwt;
mod keys;
mod state;

use anyhow::Result;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

use crate::keys::KeyMaterial;
use crate::state::CodeStore;

/// Effective configuration. Hard-coded by design — this is a dev tool.
pub struct Config {
    pub issuer: String,
    pub bind_addr: SocketAddr,
    pub client_id: String,
    pub default_user_sub: String,
    pub default_user_email: String,
    pub default_user_name: String,
}

impl Default for Config {
    fn default() -> Self {
        let port: u16 = std::env::var("DEV_IDP_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5556);
        Self {
            issuer: format!("http://localhost:{}", port),
            bind_addr: SocketAddr::from(([127, 0, 0, 1], port)),
            client_id: "noye-local-client".to_string(),
            default_user_sub: "local-admin-1".to_string(),
            default_user_email: std::env::var("DEV_IDP_USER_EMAIL")
                .unwrap_or_else(|_| "admin@local.test".to_string()),
            default_user_name: std::env::var("DEV_IDP_USER_NAME")
                .unwrap_or_else(|_| "Local Admin".to_string()),
        }
    }
}

pub struct AppState {
    pub config: Config,
    pub keys: KeyMaterial,
    pub codes: CodeStore,
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::default();
    let bind = config.bind_addr;
    let issuer = config.issuer.clone();
    let email = config.default_user_email.clone();

    let state = Arc::new(AppState {
        config,
        keys: KeyMaterial::fresh()?,
        codes: CodeStore::new(),
    });

    eprintln!("noye-dev-idp listening on {}", bind);
    eprintln!("  issuer:        {}", issuer);
    eprintln!("  default user:  {} (sub=local-admin-1)", email);
    eprintln!();
    eprintln!("Configure crates/gateway/wrangler.toml with:");
    eprintln!("  OIDC_ISSUER_URL    = \"{}\"", issuer);
    eprintln!("  OIDC_CLIENT_ID     = \"noye-local-client\"");
    eprintln!("  OIDC_REDIRECT_URI  = \"http://localhost:8787/auth/callback\"");
    eprintln!();

    let listener = TcpListener::bind(bind).await?;
    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);
        let state = Arc::clone(&state);

        tokio::spawn(async move {
            let svc = service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { handlers::dispatch(req, state).await }
            });
            if let Err(err) = http1::Builder::new().serve_connection(io, svc).await {
                eprintln!("connection error: {}", err);
            }
        });
    }
}
