//! KV-backed session store.
//!
//! The session ID is a cryptographically random 256-bit value, base64url-encoded into a cookie.
//! The session body (user info, expiry, etc.) is stored in KV with a TTL.

use serde::{Deserialize, Serialize};
use worker::*;

use super::cookie;
use super::crypto;

const SESSION_KEY_PREFIX: &str = "session:";
const DEFAULT_COOKIE_NAME: &str = "noye_session";
const DEFAULT_DURATION_MIN: i64 = 480; // 8 時間

/// Persistent session created after login.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub user_email: String,
    pub user_sub: String, // OIDC sub クレーム (IdP 内の一意識別子)
    pub issued_at: i64,
    pub expires_at: i64,
    /// Synchronizer-pattern CSRF token bound to this session. Generated at
    /// session creation time, returned to the browser via a `<meta>` tag in
    /// every authenticated page, and required as `X-CSRF-Token` on every
    /// state-changing request. Cleared automatically when the session is
    /// destroyed (logout / TTL expiry).
    ///
    /// Optional in the deserialized struct so sessions issued before the
    /// CSRF rollout can still load — those sessions will simply skip the
    /// CSRF check (see `lib::verify_csrf_for_state_changing_request`) and
    /// will get a token attached on their next renewal.
    #[serde(default)]
    pub csrf_token: Option<String>,
}

/// State held temporarily while an authorization request is in flight.
///
/// It is matched against the state parameter when the callback arrives, so
/// we store it in KV under a short TTL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingLogin {
    pub state: String,
    pub nonce: String,
    pub pkce_verifier: String,
    pub return_to: String,
    pub created_at: i64,
}

pub fn cookie_name(env: &Env) -> String {
    env.var("SESSION_COOKIE_NAME")
        .ok()
        .map(|v| v.to_string())
        .unwrap_or_else(|| DEFAULT_COOKIE_NAME.to_string())
}

fn duration_minutes(env: &Env) -> i64 {
    env.var("SESSION_DURATION_MIN")
        .ok()
        .and_then(|v| v.to_string().parse::<i64>().ok())
        .unwrap_or(DEFAULT_DURATION_MIN)
}

/// Create a new session and persist it to KV.
pub async fn create(env: &Env, user_email: &str, user_sub: &str) -> Result<(Session, String)> {
    let raw = crypto::random_bytes(32)
        .map_err(|e| Error::RustError(format!("session id generation failed: {}", e)))?;
    let session_id = crypto::base64url_encode(&raw);

    // Generate the CSRF token alongside the session ID. Same entropy
    // source, same lifetime — they live and die together.
    let csrf_token = super::csrf::generate()
        .map_err(|e| Error::RustError(format!("csrf token generation failed: {}", e)))?;

    let now = chrono::Utc::now().timestamp();
    let duration = duration_minutes(env) * 60;

    let session = Session {
        session_id: session_id.clone(),
        user_email: user_email.to_string(),
        user_sub: user_sub.to_string(),
        issued_at: now,
        expires_at: now + duration,
        csrf_token: Some(csrf_token),
    };

    let kv = env.kv("CACHE_KV")?;
    let serialized = serde_json::to_string(&session)
        .map_err(|e| Error::RustError(format!("session serialize error: {}", e)))?;

    kv.put(
        &format!("{}{}", SESSION_KEY_PREFIX, session_id),
        &serialized,
    )?
    .expiration_ttl(duration as u64)
    .execute()
    .await?;

    // Build the cookie value. In development, drop the Secure attribute so
    // a plain-HTTP `wrangler dev` session (`http://localhost:8787`) can
    // round-trip the cookie. Production keeps Secure on, which Cloudflare's
    // HTTPS-only edge satisfies.
    let secure = !crate::env_check::Environment::from_env(env).is_development();
    let cookie_value = cookie::CookieBuilder::new(cookie_name(env), &session_id)
        .max_age(duration)
        .secure(secure)
        .build();

    Ok((session, cookie_value))
}

/// Load a session from the cookie.
///
/// Returns `None` if the cookie is missing, the KV entry is missing, or the session has expired.
pub async fn load_from_cookie(req: &Request, env: &Env) -> Result<Option<Session>> {
    let name = cookie_name(env);
    let session_id = match cookie::get(req, &name)? {
        Some(v) => v,
        None => return Ok(None),
    };

    let kv = env.kv("CACHE_KV")?;
    let raw = match kv
        .get(&format!("{}{}", SESSION_KEY_PREFIX, session_id))
        .text()
        .await?
    {
        Some(v) => v,
        None => return Ok(None),
    };

    let session: Session = serde_json::from_str(&raw)
        .map_err(|e| Error::RustError(format!("session deserialize error: {}", e)))?;

    // Expiry check (KV TTL should already remove it, but double-check)
    let now = chrono::Utc::now().timestamp();
    if session.expires_at < now {
        return Ok(None);
    }

    Ok(Some(session))
}

/// Delete the session from KV (used on logout).
pub async fn destroy(env: &Env, session_id: &str) -> Result<()> {
    let kv = env.kv("CACHE_KV")?;
    kv.delete(&format!("{}{}", SESSION_KEY_PREFIX, session_id))
        .await?;
    Ok(())
}

/// Return a cookie header value that erases the session cookie.
///
/// Mirrors the `Secure` attribute used by `create()` so the browser actually
/// matches and clears the cookie. (Browsers compare cookie attributes when
/// deciding whether two Set-Cookie headers refer to the same cookie.)
pub fn clear_cookie(env: &Env) -> String {
    let secure = !crate::env_check::Environment::from_env(env).is_development();
    cookie::CookieBuilder::expired(cookie_name(env))
        .secure(secure)
        .build()
}

// ─────────────────────────────────────────────
//  Active-session enumeration (for /me/security)
// ─────────────────────────────────────────────

/// Maximum keys we list per page. Cloudflare KV's hard cap is 1000; we keep
/// the same limit. For a small monitoring tool the per-user count is
/// typically 1-3, and total active sessions across all users rarely exceeds
/// a few dozen, so a single page is enough in practice. If a deployment
/// ever exceeds 1000 active sessions we'd want a per-user index, but until
/// then this is the simpler shape.
const SESSION_LIST_PAGE_SIZE: u64 = 1000;

/// List every active session belonging to `user_email`.
///
/// Implementation: list `session:*` keys, fetch each one, deserialize, and
/// keep those whose `user_email` matches. Result is unsorted; the UI sorts
/// by `issued_at` for display.
///
/// Cost: 1 KV `list` + N KV `get` calls where N is the page size. For the
/// expected scale (< 100 active sessions globally) this is cheap. Page-2+
/// is intentionally not handled — if it's ever needed, the layout above
/// signposts where to add cursor pagination.
pub async fn list_active_for_user(env: &Env, user_email: &str) -> Result<Vec<Session>> {
    let kv = env.kv("CACHE_KV")?;
    let listed = kv
        .list()
        .prefix(SESSION_KEY_PREFIX.to_string())
        .limit(SESSION_LIST_PAGE_SIZE)
        .execute()
        .await?;

    let mut out = Vec::new();
    for key in listed.keys {
        // Best-effort: KV reads can fail individually (e.g. concurrent
        // expiry). Skip errors rather than failing the whole page.
        if let Ok(Some(raw)) = kv.get(&key.name).text().await
            && let Ok(session) = serde_json::from_str::<Session>(&raw)
            && session.user_email == user_email
        {
            out.push(session);
        }
    }
    Ok(out)
}

/// Pure-logic helper: from a list of sessions, return the IDs to revoke
/// when "log out everywhere else" is requested. Excludes `current_id`.
///
/// Pulled out as a free function so the exclusion logic is unit-tested
/// without a worker runtime.
pub fn ids_to_revoke_excluding_current<'a>(
    sessions: &'a [Session],
    current_id: &str,
) -> Vec<&'a str> {
    sessions
        .iter()
        .map(|s| s.session_id.as_str())
        .filter(|id| *id != current_id)
        .collect()
}

/// Revoke every session belonging to `user_email` except `current_id`.
///
/// Returns the number of sessions actually deleted. Failures of individual
/// `delete` calls are ignored (best-effort) — a missing key is fine, and a
/// transient KV error on one key shouldn't block the others.
pub async fn revoke_others_for_user(
    env: &Env,
    user_email: &str,
    current_id: &str,
) -> Result<usize> {
    let sessions = list_active_for_user(env, user_email).await?;
    let to_revoke = ids_to_revoke_excluding_current(&sessions, current_id);
    let mut revoked = 0usize;
    for sid in to_revoke {
        // destroy() returns Result; we accept Err silently per the
        // best-effort contract above.
        if destroy(env, sid).await.is_ok() {
            revoked += 1;
        }
    }
    Ok(revoked)
}

// ─────────────────────────────────────────────
//  PendingLogin (transient state for an in-flight authorization request)
// ─────────────────────────────────────────────

const PENDING_KEY_PREFIX: &str = "pending_login:";
const PENDING_TTL_SEC: u64 = 600; // 10 minutes

pub async fn save_pending(env: &Env, pending: &PendingLogin) -> Result<()> {
    let kv = env.kv("CACHE_KV")?;
    let serialized = serde_json::to_string(pending)
        .map_err(|e| Error::RustError(format!("pending serialize error: {}", e)))?;
    kv.put(
        &format!("{}{}", PENDING_KEY_PREFIX, pending.state),
        &serialized,
    )?
    .expiration_ttl(PENDING_TTL_SEC)
    .execute()
    .await?;
    Ok(())
}

pub async fn consume_pending(env: &Env, state: &str) -> Result<Option<PendingLogin>> {
    let kv = env.kv("CACHE_KV")?;
    let key = format!("{}{}", PENDING_KEY_PREFIX, state);
    let raw = match kv.get(&key).text().await? {
        Some(v) => v,
        None => return Ok(None),
    };
    // Single-use: delete on consumption
    let _ = kv.delete(&key).await;

    let pending: PendingLogin = serde_json::from_str(&raw)
        .map_err(|e| Error::RustError(format!("pending deserialize error: {}", e)))?;
    Ok(Some(pending))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake(id: &str, email: &str) -> Session {
        Session {
            session_id: id.to_string(),
            user_email: email.to_string(),
            user_sub: "sub".to_string(),
            issued_at: 0,
            expires_at: 0,
            csrf_token: None,
        }
    }

    #[test]
    fn ids_to_revoke_excludes_only_the_current_session() {
        let sessions = vec![fake("a", "u@x"), fake("b", "u@x"), fake("c", "u@x")];
        let to_revoke = ids_to_revoke_excluding_current(&sessions, "b");
        assert_eq!(to_revoke, vec!["a", "c"]);
    }

    #[test]
    fn ids_to_revoke_returns_all_when_current_is_unknown() {
        // Current session might be expired or simply not in the list (e.g.
        // listing happened just after the user clicked "log out everywhere
        // else" from a session that was about to TTL). In that case every
        // listed session is a candidate to revoke — there is nothing to
        // preserve.
        let sessions = vec![fake("a", "u@x"), fake("b", "u@x")];
        let to_revoke = ids_to_revoke_excluding_current(&sessions, "nonexistent");
        assert_eq!(to_revoke, vec!["a", "b"]);
    }

    #[test]
    fn ids_to_revoke_empty_input_returns_empty() {
        let sessions: Vec<Session> = Vec::new();
        let to_revoke = ids_to_revoke_excluding_current(&sessions, "a");
        assert!(to_revoke.is_empty());
    }

    #[test]
    fn ids_to_revoke_with_only_current_returns_empty() {
        // The single session is the current one — nothing else to revoke.
        let sessions = vec![fake("a", "u@x")];
        let to_revoke = ids_to_revoke_excluding_current(&sessions, "a");
        assert!(to_revoke.is_empty());
    }

    #[test]
    fn ids_to_revoke_preserves_input_order() {
        // The UI shows "X sessions revoked" without listing them; still,
        // a stable order makes any future debugging easier.
        let sessions = vec![fake("zzz", "u@x"), fake("aaa", "u@x"), fake("mmm", "u@x")];
        let to_revoke = ids_to_revoke_excluding_current(&sessions, "aaa");
        assert_eq!(to_revoke, vec!["zzz", "mmm"]);
    }
}
