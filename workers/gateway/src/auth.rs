pub mod cookie;
pub mod crypto;
pub mod jwks;
pub mod jwt;
pub mod oidc;
pub mod rbac;
pub mod session;

// Gateway では Caller を noye_shared から再エクスポートする
pub use noye_shared::Caller;

use worker::*;

use crate::core_client;

/// リクエストからセッション Cookie を読み、Core 経由で users と突き合わせて Caller を構築する。
///
/// # 認証失敗の取り扱い
/// - Cookie なし / 無効 → UNAUTHORIZED
/// - KV にセッション無し → UNAUTHORIZED
/// - Core 照会で未登録 / 非アクティブ → FORBIDDEN (ゲスト禁止方針)
pub async fn extract_caller(req: &Request, env: &Env) -> Result<Caller> {
    let session = session::load_from_cookie(req, env)
        .await?
        .ok_or_else(|| Error::RustError("UNAUTHORIZED: no valid session".to_string()))?;

    // Core 経由で RBAC ロール等の最新情報を取得
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

/// 管理者権限を要求するガード。
pub fn require_admin(caller: &Caller) -> Result<()> {
    if caller.is_admin() {
        Ok(())
    } else {
        Err(Error::RustError(
            "FORBIDDEN: admin role required".to_string(),
        ))
    }
}

/// エラーメッセージから認証失敗系かを判定する (ミドルウェア的に使用)。
pub fn is_unauthorized(err: &Error) -> bool {
    matches!(err, Error::RustError(msg) if msg.starts_with("UNAUTHORIZED"))
}
