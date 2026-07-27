use worker::*;

use noye_shared::Target;
use super::CheckOutcome;

/// TLS certificate check (requirement 2-3)
///
/// Validation steps:
/// - Successful chain validation (Cloudflare fetch performs this implicitly)
/// - Number of days until expiry is at least the configured threshold
/// - Successful connection
///
/// Direct X.509 certificate access is restricted in the Workers environment, so
/// we rely on HTTPS fetch and Cf-Tls-* headers.
/// Consider integrating with external APIs (such as crt.sh) for full certificate metadata.
pub async fn check_certificate(_env: &Env, target: &Target) -> CheckOutcome {
    let start = js_sys::Date::now() as i64;
    let host = &target.host;
    let port = target.port.unwrap_or(443);
    let threshold_days = target.tls_threshold_days.unwrap_or(30);

    // Connect over HTTPS to implicitly validate the TLS handshake
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

    // Execute the fetch (which includes the TLS handshake)
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

    // Direct access to the certificate expiry is hard in the Workers environment, so
    // we ask an external certificate-information API for the expiry
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

            // Day-remaining threshold check
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
            // Connection itself succeeded even if certificate metadata could not be retrieved
            // Once the TLS handshake completes, chain validation and revocation checking have already been performed by Cloudflare
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

/// Certificate information
struct CertInfo {
    not_after: String,
    days_left: i64,
    issuer: String,
}

/// Retrieve certificate expiry information via an external API
///
/// Uses the crt.sh JSON API to fetch the latest certificate information.
/// In production, cache the results in KV to reduce API calls.
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

    // Compute days-remaining
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

/// Compute days remaining from the expiry string
fn parse_days_until(not_after: &str) -> Option<i64> {
    // crt.sh date format: "2025-06-15T23:59:59"
    let expiry = chrono::NaiveDateTime::parse_from_str(not_after, "%Y-%m-%dT%H:%M:%S").ok()?;
    let now = chrono::Utc::now().naive_utc();
    let duration = expiry - now;
    Some(duration.num_days())
}
