//! Core Worker の内部 API 層。
//!
//! Gateway からの Service Binding 呼び出し (HTTP) のみを受け付ける。
//! ルート Worker ではない (wrangler.toml で `workers_dev = false`) ため、
//! 外部からは到達不能。二重防御として `X-Gateway-Token` ヘッダも検証する。

pub mod audit;
pub mod incidents;
pub mod maintenance;
pub mod targets;
pub mod users;

use noye_shared::{header, Caller};
use worker::*;

/// 管理者ガード。
pub fn require_admin(caller: &Caller) -> Result<()> {
    if caller.is_admin() {
        Ok(())
    } else {
        Err(Error::RustError("FORBIDDEN: admin required".to_string()))
    }
}

/// Gateway 共有秘密の検証。
///
/// ルートを持たない Core は既に外部遮断されているが、万一の構成ミス
/// (ルートの誤設定、Service Binding 乗っ取り等) に備えた二重防御。
pub fn verify_gateway_token_env(req: &Request, env: &Env) -> Result<()> {
    let expected = match env.secret("GATEWAY_SHARED_TOKEN") {
        Ok(v) => v.to_string(),
        Err(_) => match env.var("GATEWAY_SHARED_TOKEN") {
            Ok(v) => v.to_string(),
            Err(_) => return Ok(()), // 未設定ならスキップ (開発時)
        },
    };
    let got = req.headers().get(header::GATEWAY_TOKEN)?;
    if got.as_deref() != Some(expected.as_str()) {
        return Err(Error::RustError(
            "FORBIDDEN: invalid gateway token".to_string(),
        ));
    }
    Ok(())
}

/// リクエストから Gateway が注入した呼び出し元情報を取り出す。
pub fn require_caller_with_env(req: &Request, env: &Env) -> Result<Caller> {
    verify_gateway_token_env(req, env)?;

    let h = req.headers();
    let user_id = h
        .get(header::CALLER_USER_ID)?
        .ok_or_else(|| Error::RustError("Missing X-Caller-UserId".to_string()))?;
    let email = h
        .get(header::CALLER_EMAIL)?
        .ok_or_else(|| Error::RustError("Missing X-Caller-Email".to_string()))?;
    let name = h.get(header::CALLER_NAME)?.unwrap_or_else(|| email.clone());
    let role = h
        .get(header::CALLER_ROLE)?
        .ok_or_else(|| Error::RustError("Missing X-Caller-Role".to_string()))?;

    Ok(Caller { user_id, email, name, role })
}
