use worker::*;

use noye_shared::Target;
use super::CheckOutcome;

/// TLS 証明書チェック (要件2-3)
///
/// 検証項目:
/// - チェーン検証成功 (Cloudflare の fetch が暗黙的に検証)
/// - 有効期限の残日数がしきい値以上
/// - 接続成功
///
/// Workers 環境では直接的な X.509 証明書アクセスが制限されるため、
/// HTTPS fetch + Cf-Tls-* ヘッダ情報を活用する。
/// 証明書の詳細情報取得には外部 API (crt.sh 等) との連携も検討。
pub async fn check_certificate(_env: &Env, target: &Target) -> CheckOutcome {
    let start = js_sys::Date::now() as i64;
    let host = &target.host;
    let port = target.port.unwrap_or(443);
    let threshold_days = target.tls_threshold_days.unwrap_or(30);

    // HTTPS で接続し、TLS ハンドシェイクを暗黙的に検証
    let url = format!("https://{}:{}/", host, port);

    let mut init = RequestInit::new();
    init.with_method(Method::Head);

    let request = match Request::new_with_init(&url, &init) {
        Ok(r) => r,
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            return CheckOutcome::failure(
                format!("TLS request build error: {:?}", e),
                elapsed,
            );
        }
    };

    // Fetch 実行 (TLSハンドシェイク含む)
    let response = match Fetch::Request(request).send().await {
        Ok(r) => r,
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            return CheckOutcome::failure(
                format!("TLS connection failed: {:?}", e),
                elapsed,
            );
        }
    };

    let elapsed = (js_sys::Date::now() as i64) - start;

    // Workers 環境では証明書の有効期限を直接取得するのが難しいため、
    // 外部の証明書情報 API を利用して有効期限を確認する
    match fetch_cert_expiry(host).await {
        Ok(cert_info) => {
            let mut outcome = CheckOutcome {
                is_success: true,
                status_code: Some(response.status_code() as i64),
                response_time_ms: elapsed,
                error_message: None,
                tls_expiry_date: Some(cert_info.not_after.clone()),
                tls_days_left: Some(cert_info.days_left),
                details: Some(format!(
                    "TLS OK: expires {} ({} days left), issuer: {}",
                    cert_info.not_after, cert_info.days_left, cert_info.issuer
                )),
            };

            // 残日数しきい値チェック
            if cert_info.days_left < threshold_days {
                outcome.is_success = false;
                outcome.error_message = Some(format!(
                    "TLS certificate expires in {} days (threshold: {} days)",
                    cert_info.days_left, threshold_days
                ));
            }

            outcome
        }
        Err(e) => {
            // 証明書情報取得失敗でも接続自体は成功
            // TLS ハンドシェイクが成功した時点で、チェーン検証と失効チェックは Cloudflare が行っている
            CheckOutcome {
                is_success: true,
                status_code: Some(response.status_code() as i64),
                response_time_ms: elapsed,
                error_message: None,
                tls_expiry_date: None,
                tls_days_left: None,
                details: Some(format!(
                    "TLS handshake OK but cert details unavailable: {}",
                    e
                )),
            }
        }
    }
}

/// 証明書情報
struct CertInfo {
    not_after: String,
    days_left: i64,
    issuer: String,
}

/// 証明書の有効期限情報を外部 API 経由で取得
///
/// crt.sh の JSON API を利用して最新の証明書情報を取得する。
/// 本番環境では結果を KV にキャッシュして API 呼び出しを削減する。
async fn fetch_cert_expiry(host: &str) -> std::result::Result<CertInfo, String> {
    let url = format!(
        "https://crt.sh/?q={}&output=json&limit=1",
        host
    );

    let mut init = RequestInit::new();
    init.with_method(Method::Get);

    let request = Request::new_with_init(&url, &init)
        .map_err(|e| format!("crt.sh request error: {:?}", e))?;

    let mut response = Fetch::Request(request)
        .send()
        .await
        .map_err(|e| format!("crt.sh fetch error: {:?}", e))?;

    let text = response
        .text()
        .await
        .map_err(|e| format!("crt.sh read error: {:?}", e))?;

    let entries: Vec<serde_json::Value> = serde_json::from_str(&text)
        .map_err(|e| format!("crt.sh parse error: {}", e))?;

    let entry = entries.first().ok_or("No certificate found on crt.sh")?;

    let not_after = entry
        .get("not_after")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let issuer = entry
        .get("issuer_name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // 残日数を計算
    let days_left = if not_after != "unknown" {
        parse_days_until(&not_after).unwrap_or(0)
    } else {
        0
    };

    Ok(CertInfo {
        not_after,
        days_left,
        issuer,
    })
}

/// 有効期限文字列から残日数を計算
fn parse_days_until(not_after: &str) -> Option<i64> {
    // crt.sh の日付形式: "2025-06-15T23:59:59"
    let expiry = chrono::NaiveDateTime::parse_from_str(not_after, "%Y-%m-%dT%H:%M:%S").ok()?;
    let now = chrono::Utc::now().naive_utc();
    let duration = expiry - now;
    Some(duration.num_days())
}
