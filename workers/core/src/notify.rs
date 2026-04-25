pub mod channels;

use worker::*;

use noye_shared::Target;
use crate::monitor::CheckOutcome;

/// 障害通知のディスパッチ
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
        if let Err(e) = send_notification(&channel.channel_type, &channel.endpoint, &message).await {
            console_error!("Failed to send DOWN notification via {}: {:?}", channel.channel_type, e);
        }
    }
}

/// 復旧通知のディスパッチ
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
        if let Err(e) = send_notification(&channel.channel_type, &channel.endpoint, &message).await {
            console_error!("Failed to send UP notification via {}: {:?}", channel.channel_type, e);
        }
    }
}

async fn send_notification(channel_type: &str, endpoint: &str, message: &NotificationMessage) -> Result<()> {
    match channel_type {
        "webhook" => send_webhook(endpoint, message).await,
        "slack" => send_slack(endpoint, message).await,
        "email" => {
            console_log!("Email notification to {}: {}", endpoint, message.title);
            Ok(())
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
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    init.with_headers(headers);

    let request = Request::new_with_init(url, &init)?;
    Fetch::Request(request).send().await?;
    Ok(())
}

async fn send_slack(webhook_url: &str, message: &NotificationMessage) -> Result<()> {
    let color = if message.status == "down" { "#dc3545" } else { "#28a745" };
    let emoji = if message.status == "down" { ":red_circle:" } else { ":large_green_circle:" };

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
    let mut headers = Headers::new();
    headers.set("Content-Type", "application/json")?;
    init.with_headers(headers);

    let request = Request::new_with_init(webhook_url, &init)?;
    Fetch::Request(request).send().await?;
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
