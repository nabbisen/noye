pub mod cookie;
pub mod crypto;
pub mod csrf;
pub mod jwks;
pub mod jwt;
pub mod oidc;
pub mod rbac;
pub mod session;
pub mod turnstile;

// The Gateway re-exports Caller from noye_shared
pub use noye_shared::Caller;

use worker::*;

use crate::core_client;

/// Read the session cookie from the request and resolve a Caller by looking up the user via the Core.
///
/// # Authentication失敗の取り扱い
/// - No cookie / invalid cookie -> UNAUTHORIZED
/// - Session not present in KV -> UNAUTHORIZED
/// - Lookup via Core finds no entry / inactive user -> FORBIDDEN (guests are not allowed)
pub async fn extract_caller(req: &Request, env: &Env) -> Result<Caller> {
    let session = session::load_from_cookie(req, env)
        .await?
        .ok_or_else(|| Error::RustError("UNAUTHORIZED: no valid session".to_string()))?;

    // Look up the latest RBAC role and user metadata via the Core
    let user = core_client::lookup_user(env, &session.user_email)
        .await?
        .ok_or_else(|| {
            Error::RustError(format!(
                "FORBIDDEN: user not registered or inactive: {}",
                session.user_email
            ))
        })?;

    if !user.is_active {
        return Err(Error::RustError(format!(
            "FORBIDDEN: user inactive: {}",
            session.user_email
        )));
    }

    Ok(Caller {
        user_id: user.id,
        email: user.email,
        name: user.name,
        role: user.role,
    })
}

/// Guard that requires administrator role.
pub fn require_admin(caller: &Caller) -> Result<()> {
    if caller.is_admin() {
        Ok(())
    } else {
        Err(Error::RustError(
            "FORBIDDEN: admin role required".to_string(),
        ))
    }
}

/// エラーメッセージからAuthentication失敗系かを判定する (ミドルウェア的に使用)。
pub fn is_unauthorized(err: &Error) -> bool {
    matches!(err, Error::RustError(msg) if msg.starts_with("UNAUTHORIZED"))
}
