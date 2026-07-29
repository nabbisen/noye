use worker::*;

use super::CheckOutcome;
use noye_shared::Target;

/// HTTP/HTTPS health check (requirement 2-3)
///
/// Validation steps:
/// - connection establishment
/// - No timeout
/// - The expected status code (e.g. 2xx)
/// - Body-content validation against `body_contains`
pub async fn check(_env: &Env, target: &Target) -> CheckOutcome {
    let start = js_sys::Date::now() as i64;

    // Build the URL
    let scheme = &target.target_type; // "http" or "https"
    let port_suffix = match target.port {
        Some(80) if scheme == "http" => String::new(),
        Some(443) if scheme == "https" => String::new(),
        Some(p) => format!(":{}", p),
        None => String::new(),
    };
    let path = target.path.as_deref().unwrap_or("/");
    let url = format!("{}://{}{}{}", scheme, target.host, port_suffix, path);

    // Build the Fetch request
    let mut init = RequestInit::new();
    init.with_method(Method::Get);

    // An AbortController for timeout handling is unavailable in Workers, so
    // we approximate it via signal-based timeouts
    let request = match Request::new_with_init(&url, &init) {
        Ok(r) => r,
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            return CheckOutcome::failure(format!("Request build error: {:?}", e), elapsed);
        }
    };

    // Execute the fetch
    let response = match Fetch::Request(request).send().await {
        Ok(r) => r,
        Err(e) => {
            let elapsed = (js_sys::Date::now() as i64) - start;
            return CheckOutcome::failure(format!("Fetch error: {:?}", e), elapsed);
        }
    };

    let elapsed = (js_sys::Date::now() as i64) - start;

    // Timeout decision
    let timeout_ms = target.timeout_sec * 1000;
    if elapsed > timeout_ms {
        return CheckOutcome::failure(
            format!("Timeout: {}ms > {}ms limit", elapsed, timeout_ms),
            elapsed,
        );
    }

    // Status-code validation
    let status = response.status_code() as i64;
    let expected = target.expected_status.unwrap_or(200);

    // Range check for 2xx (200-299)
    let status_ok = if (200..300).contains(&expected) {
        (200..300).contains(&status)
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

    // Body-content string validation against `body_contains`
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

    // All checks passed
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
