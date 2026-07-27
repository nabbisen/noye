pub mod channels;
pub mod email;

use worker::*;

use noye_shared::{NotificationChannel, Target};
use crate::monitor::CheckOutcome;

/// Dispatch a down notification
pub async fn dispatch_down(env: &Env, target: &Target, outcome: &CheckOutcome) {
    let db = match env.d1("DB") {
        Ok(db) => db,
        Err(_) => return,
    };

    let channel_list = match channels::get_channels_for_target(&db, &target.id).await {
        Ok(c) => c,
        Err(e) => {
            console_error!("Failed to fetch notification channels: {:?}", e);
            return;
        }
    };

    let message = format_down_message(target, outcome);

    for channel in &channel_list {
        if !channel.on_down {
            continue;
        }
        if let Err(e) = send_notification(env, &channel.channel_type, &channel.endpoint, &message).await {
            console_error!("Failed to send DOWN notification via {}: {:?}", channel.channel_type, e);
        }
    }
}

/// Dispatch a recovery notification
pub async fn dispatch_up(env: &Env, target: &Target) {
    let db = match env.d1("DB") {
        Ok(db) => db,
        Err(_) => return,
    };

    let channel_list = match channels::get_channels_for_target(&db, &target.id).await {
        Ok(c) => c,
        Err(e) => {
            console_error!("Failed to fetch notification channels: {:?}", e);
            return;
        }
    };

    let message = format_up_message(target);

    for channel in &channel_list {
        if !channel.on_up {
            continue;
        }
        if let Err(e) = send_notification(env, &channel.channel_type, &channel.endpoint, &message).await {
            console_error!("Failed to send UP notification via {}: {:?}", channel.channel_type, e);
        }
    }
}

/// Send a single test notification to the given channel.
///
/// Returns `Ok(())` only if the channel is enabled and the underlying
/// transport (HTTPS POST for webhook/slack, SMTP for email) succeeded.
///
/// Email is the special case: when the deployment has no SMTP configured the
/// test send returns a "not configured" error rather than silently
/// succeeding, and when SMTP is configured but `wasm-smtp` has not yet
/// landed it returns the gated-on-crate error from `email::send_email`.
/// Either way the operator sees something useful in the UI; "Email channel
/// silently appears to work" is the failure mode we are most worried about.
pub async fn send_test(env: &Env, channel: &NotificationChannel) -> Result<()> {
    if !channel.is_enabled {
        return Err(Error::RustError(
            "Channel is disabled; enable it before sending a test notification".to_string(),
        ));
    }
    if channel.channel_type == "email" {
        // Surface a precise reason instead of forwarding to send_notification
        // (which would log "would have sent" for the disabled case — fine
        // for cron, wrong for an interactive test send).
        let status = email::load_config(env);
        if !matches!(status, email::ConfigStatus::Ok(_)) {
            return Err(Error::RustError(email::status_message(&status)));
        }
    }
    let message = format_test_message(channel);
    send_notification(env, &channel.channel_type, &channel.endpoint, &message).await
}

async fn send_notification(
    env: &Env,
    channel_type: &str,
    endpoint: &str,
    message: &NotificationMessage,
) -> Result<()> {
    match channel_type {
        "webhook" => send_webhook(endpoint, message).await,
        "slack" => send_slack(endpoint, message).await,
        "email" => {
            // The whole transport (config inspection + send) is delegated to
            // notify::email. That module is the one place that needs to flip
            // when the wasm-smtp crate becomes available.
            match email::load_config(env) {
                email::ConfigStatus::Disabled => {
                    console_log!(
                        "[email-disabled] Skipping email to {}; configure EMAIL_SMTP_HOST to enable",
                        endpoint
                    );
                    Ok(())
                }
                email::ConfigStatus::Misconfigured(why) => {
                    Err(Error::RustError(format!(
                        "Email channel misconfigured: {}",
                        why
                    )))
                }
                email::ConfigStatus::Ok(cfg) => {
                    email::send_email(&cfg, endpoint, &message.title, &message.body).await
                }
            }
        }
        _ => Ok(()),
    }
}

async fn send_webhook(url: &str, message: &NotificationMessage) -> Result<()> {
    let payload = serde_json::json!({
        "title": message.title,
        "body": message.body,
        "status": message.status,
        "target_name": message.target_name,
        "target_host": message.target_host,
        "timestamp": message.timestamp,
    });

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(wasm_bindgen::JsValue::from_str(
        &serde_json::to_string(&payload).unwrap_or_default(),
    )));
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    init.with_headers(headers);

    let request = Request::new_with_init(url, &init)?;
    let mut response = Fetch::Request(request).send().await?;
    let status = response.status_code();
    if status < 200 || status >= 300 {
        let body = response.text().await.unwrap_or_default();
        return Err(Error::RustError(format!("webhook returned HTTP {}: {}", status, body)));
    }
    Ok(())
}

async fn send_slack(webhook_url: &str, message: &NotificationMessage) -> Result<()> {
    let color = match message.status.as_str() {
        "down" => "#dc3545",
        "test" => "#6c757d",
        _ => "#28a745",
    };
    let emoji = match message.status.as_str() {
        "down" => ":red_circle:",
        "test" => ":wrench:",
        _ => ":large_green_circle:",
    };

    let payload = serde_json::json!({
        "attachments": [{
            "color": color,
            "blocks": [{
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": format!("{} *{}*\n{}", emoji, message.title, message.body)
                }
            }, {
                "type": "context",
                "elements": [{
                    "type": "mrkdwn",
                    "text": format!("Host: {} | {}", message.target_host, message.timestamp)
                }]
            }]
        }]
    });

    let mut init = RequestInit::new();
    init.with_method(Method::Post);
    init.with_body(Some(wasm_bindgen::JsValue::from_str(
        &serde_json::to_string(&payload).unwrap_or_default(),
    )));
    let headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    init.with_headers(headers);

    let request = Request::new_with_init(webhook_url, &init)?;
    let mut response = Fetch::Request(request).send().await?;
    let status = response.status_code();
    if status < 200 || status >= 300 {
        let body = response.text().await.unwrap_or_default();
        return Err(Error::RustError(format!("slack webhook returned HTTP {}: {}", status, body)));
    }
    Ok(())
}

struct NotificationMessage {
    title: String,
    body: String,
    status: String,
    target_name: String,
    target_host: String,
    timestamp: String,
}

fn format_down_message(target: &Target, outcome: &CheckOutcome) -> NotificationMessage {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    NotificationMessage {
        title: format!("[DOWN] {} is unreachable", target.name),
        body: format!(
            "Target {} ({}) is down.\nError: {}\nResponse time: {}ms",
            target.name, target.host,
            outcome.error_message.as_deref().unwrap_or("Unknown"),
            outcome.response_time_ms,
        ),
        status: "down".to_string(),
        target_name: target.name.clone(),
        target_host: target.host.clone(),
        timestamp: now,
    }
}

fn format_up_message(target: &Target) -> NotificationMessage {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    NotificationMessage {
        title: format!("[UP] {} has recovered", target.name),
        body: format!("Target {} ({}) is back online.", target.name, target.host),
        status: "up".to_string(),
        target_name: target.name.clone(),
        target_host: target.host.clone(),
        timestamp: now,
    }
}

/// Build a synthetic message for a test send. Not tied to any target — the
/// "target_*" fields are filled with the channel's metadata so receivers can
/// tell at a glance that this came from a manual test action.
fn format_test_message(channel: &NotificationChannel) -> NotificationMessage {
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    NotificationMessage {
        title: format!("[TEST] Noye notification test for channel \"{}\"", channel.name),
        body: format!(
            "This is a test message dispatched from Noye. If you are reading this, the channel \"{}\" ({}) is correctly wired and reachable. No action is required.",
            channel.name, channel.channel_type,
        ),
        status: "test".to_string(),
        target_name: format!("(test) {}", channel.name),
        target_host: format!("channel-id:{}", channel.id),
        timestamp: now,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_target() -> Target {
        Target {
            id: "t1".into(),
            name: "API".into(),
            target_type: "https".into(),
            host: "api.example.com".into(),
            port: None,
            path: Some("/health".into()),
            expected_status: Some(200),
            body_contains: None,
            tls_threshold_days: Some(30),
            timeout_sec: 10,
            retry_count: 3,
            interval_minutes: 5,
            is_disabled: false,
            owner_id: "u1".into(),
            tags: None,
            next_check_at: "2026-04-27T00:00:00Z".into(),
            created_at: "2026-04-01T00:00:00Z".into(),
            updated_at: "2026-04-01T00:00:00Z".into(),
        }
    }

    fn sample_channel(channel_type: &str, enabled: bool) -> NotificationChannel {
        NotificationChannel {
            id: "ch1".into(),
            name: "Ops Slack".into(),
            channel_type: channel_type.into(),
            endpoint: "https://hooks.slack.com/services/T/B/X".into(),
            is_enabled: enabled,
            owner_id: "u1".into(),
            created_at: "2026-04-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn test_message_uses_test_status() {
        let ch = sample_channel("slack", true);
        let m = format_test_message(&ch);
        assert_eq!(m.status, "test");
        assert!(m.title.contains("[TEST]"));
        assert!(m.title.contains("Ops Slack"));
        assert!(m.body.contains("Ops Slack"));
        assert!(m.body.contains("slack"));
    }

    #[test]
    fn test_message_keeps_channel_id_traceable() {
        let ch = sample_channel("webhook", true);
        let m = format_test_message(&ch);
        // The channel id is encoded into target_host so receivers (e.g. a
        // logging webhook) can correlate the test event back to the channel.
        assert!(m.target_host.contains("ch1"));
    }

    #[test]
    fn down_message_includes_error_and_response_time() {
        let target = sample_target();
        let outcome = CheckOutcome {
            is_success: false,
            status_code: Some(503),
            response_time_ms: 1234,
            error_message: Some("connection refused".to_string()),
            tls_expiry_date: None,
            tls_days_left: None,
            details: None,
        };
        let m = format_down_message(&target, &outcome);
        assert_eq!(m.status, "down");
        assert!(m.title.starts_with("[DOWN]"));
        assert!(m.body.contains("connection refused"));
        assert!(m.body.contains("1234"));
    }

    #[test]
    fn down_message_falls_back_to_unknown_error() {
        let target = sample_target();
        let outcome = CheckOutcome {
            is_success: false,
            status_code: None,
            response_time_ms: 500,
            error_message: None,
            tls_expiry_date: None,
            tls_days_left: None,
            details: None,
        };
        let m = format_down_message(&target, &outcome);
        assert!(m.body.contains("Unknown"));
    }

    #[test]
    fn up_message_uses_up_status() {
        let target = sample_target();
        let m = format_up_message(&target);
        assert_eq!(m.status, "up");
        assert!(m.title.starts_with("[UP]"));
        assert!(m.body.contains("API"));
        assert!(m.body.contains("api.example.com"));
    }
}
