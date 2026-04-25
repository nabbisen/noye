use worker::*;

use noye_shared::Target;
use super::CheckOutcome;

/// HTTP/HTTPS ヘルスチェック (要件2-3)
///
/// 検証項目:
/// - 接続確立
/// - タイムアウト未発生
/// - 期待ステータスコード (例: 200系)
/// - レスポンス本文の特定文字列検証 (body_contains)
pub async fn check(_env: &Env, target: &Target) -> CheckOutcome {
    let start = js_sys::Date::now() as i64;

    // URL の組み立て
    let scheme = &target.target_type; // "http" or "https"
    let port_suffix = match target.port {
        Some(80) if scheme == "http" => String::new(),
        Some(443) if scheme == "https" => String::new(),
        Some(p) => format!(":{}", p),
        None => String::new(),
    };
    let path = target.path.as_deref().unwrap_or("/");
    let url = format!("{}://{}{}{}", scheme, target.host, port_suffix, path);

    // Fetch リクエストの構築
    let mut init = RequestInit::new();
    init.with_method(Method::Get);

    // タイムアウト設定のためのAbortControllerは Workers 環境では
    // signal ベースのタイムアウト制御を行う
    let request = match Request::new_with_init(&url, &init) {
        Ok(r) => r,
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            return CheckOutcome::failure(format!("Request build error: {:?}", e), elapsed);
        }
    };

    // Fetch 実行
    let response = match Fetch::Request(request).send().await {
        Ok(r) => r,
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            return CheckOutcome::failure(format!("Fetch error: {:?}", e), elapsed);
        }
    };

    let elapsed = (js_sys::Date::now() as i64) - start;

    // タイムアウト判定
    let timeout_ms = (target.timeout_sec * 1000) as i64;
    if elapsed > timeout_ms {
        return CheckOutcome::failure(
            format!("Timeout: {}ms > {}ms limit", elapsed, timeout_ms),
            elapsed,
        );
    }

    // ステータスコード検証
    let status = response.status_code() as i64;
    let expected = target.expected_status.unwrap_or(200);

    // 200系 (200-299) の範囲チェック
    let status_ok = if expected >= 200 && expected < 300 {
        status >= 200 && status < 300
    } else {
        status == expected
    };

    if !status_ok {
        return CheckOutcome {
            is_success: false,
            status_code: Some(status),
            response_time_ms: elapsed,
            error_message: Some(format!(
                "Unexpected status: got {}, expected {}",
                status, expected
            )),
            tls_expiry_date: None,
            tls_days_left: None,
            details: None,
        };
    }

    // レスポンス本文の文字列検証 (body_contains)
    if let Some(ref expected_body) = target.body_contains {
        let mut resp = response;
        match resp.text().await {
            Ok(body) => {
                if !body.contains(expected_body.as_str()) {
                    return CheckOutcome {
                        is_success: false,
                        status_code: Some(status),
                        response_time_ms: elapsed,
                        error_message: Some(format!(
                            "Body does not contain expected string: '{}'",
                            expected_body
                        )),
                        tls_expiry_date: None,
                        tls_days_left: None,
                        details: None,
                    };
                }
            }
            Err(e) => {
                return CheckOutcome {
                    is_success: false,
                    status_code: Some(status),
                    response_time_ms: elapsed,
                    error_message: Some(format!("Body read error: {:?}", e)),
                    tls_expiry_date: None,
                    tls_days_left: None,
                    details: None,
                };
            }
        }
    }

    // 全検証パス
    CheckOutcome {
        is_success: true,
        status_code: Some(status),
        response_time_ms: elapsed,
        error_message: None,
        tls_expiry_date: None,
        tls_days_left: None,
        details: Some(format!("HTTP {} in {}ms", status, elapsed)),
    }
}
