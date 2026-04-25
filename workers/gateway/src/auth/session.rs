//! KV ベースのセッションストア。
//!
//! セッション ID は暗号学的乱数 (32バイト = 256bit) を base64url エンコードし Cookie に格納。
//! セッション本体 (ユーザー情報、有効期限等) は KV に TTL 付きで保存。

use serde::{Deserialize, Serialize};
use worker::*;

use super::cookie;
use super::crypto;

const SESSION_KEY_PREFIX: &str = "session:";
const DEFAULT_COOKIE_NAME: &str = "noye_session";
const DEFAULT_DURATION_MIN: i64 = 480; // 8 時間

/// ログイン後の永続セッション。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub user_email: String,
    pub user_sub: String, // OIDC sub クレーム (IdP 内の一意識別子)
    pub issued_at: i64,
    pub expires_at: i64,
}

/// 認可リクエスト発行時に一時的に保持するステート。
///
/// callback 時に state パラメータ経由で突き合わせるため、
/// 同じ KV に短い TTL で保存する。
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

/// 新しいセッションを生成し KV に保存する。
pub async fn create(env: &Env, user_email: &str, user_sub: &str) -> Result<(Session, String)> {
    let raw = crypto::random_bytes(32)
        .map_err(|e| Error::RustError(format!("session id generation failed: {}", e)))?;
    let session_id = crypto::base64url_encode(&raw);

    let now = chrono::Utc::now().timestamp();
    let duration = duration_minutes(env) * 60;

    let session = Session {
        session_id: session_id.clone(),
        user_email: user_email.to_string(),
        user_sub: user_sub.to_string(),
        issued_at: now,
        expires_at: now + duration,
    };

    let kv = env.kv("CACHE_KV")?;
    let serialized = serde_json::to_string(&session)
        .map_err(|e| Error::RustError(format!("session serialize error: {}", e)))?;

    kv.put(&format!("{}{}", SESSION_KEY_PREFIX, session_id), &serialized)?
        .expiration_ttl(duration as u64)
        .execute()
        .await?;

    // Cookie 値を組み立て
    let cookie_value = cookie::CookieBuilder::new(cookie_name(env), &session_id)
        .max_age(duration)
        .build();

    Ok((session, cookie_value))
}

/// Cookie からセッションを読み出す。
///
/// Cookie が無い / KV にセッションが無い / 期限切れの場合は `None` を返す。
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

    // 有効期限チェック (KV の TTL で消えるはずだが念のため)
    let now = chrono::Utc::now().timestamp();
    if session.expires_at < now {
        return Ok(None);
    }

    Ok(Some(session))
}

/// セッションを KV から削除する (ログアウト時)。
pub async fn destroy(env: &Env, session_id: &str) -> Result<()> {
    let kv = env.kv("CACHE_KV")?;
    kv.delete(&format!("{}{}", SESSION_KEY_PREFIX, session_id))
        .await?;
    Ok(())
}

/// 消去用 Cookie ヘッダ値を返す。
pub fn clear_cookie(env: &Env) -> String {
    cookie::CookieBuilder::expired(cookie_name(env)).build()
}

// ─────────────────────────────────────────────
//  PendingLogin (認可リクエスト中の一時ステート)
// ─────────────────────────────────────────────

const PENDING_KEY_PREFIX: &str = "pending_login:";
const PENDING_TTL_SEC: u64 = 600; // 10 分

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
    // 使い捨て: 消費と同時に削除
    let _ = kv.delete(&key).await;

    let pending: PendingLogin = serde_json::from_str(&raw)
        .map_err(|e| Error::RustError(format!("pending deserialize error: {}", e)))?;
    Ok(Some(pending))
}
