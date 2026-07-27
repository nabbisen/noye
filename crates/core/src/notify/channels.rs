use serde::Deserialize;
use worker::*;

/// Notification channels attached to a target
pub struct TargetChannel {
    pub channel_type: String,
    pub endpoint: String,
    pub on_down: bool,
    pub on_up: bool,
}

#[derive(Deserialize)]
struct ChannelRow {
    channel_type: String,
    endpoint: String,
    on_down: i64,
    on_up: i64,
}

/// Fetch the active notification channels attached to the given target
pub async fn get_channels_for_target(
    db: &D1Database,
    target_id: &str,
) -> Result<Vec<TargetChannel>> {
    let results = db
        .prepare(
            "SELECT nc.channel_type, nc.endpoint, tn.on_down, tn.on_up
             FROM target_notifications tn
             JOIN notification_channels nc ON tn.channel_id = nc.id
             WHERE tn.target_id = ?1 AND nc.is_enabled = 1",
        )
        .bind(&[target_id.into()])?
        .all()
        .await?
        .results::<ChannelRow>()?;

    Ok(results
        .into_iter()
        .map(|r| TargetChannel {
            channel_type: r.channel_type,
            endpoint: r.endpoint,
            on_down: r.on_down != 0,
            on_up: r.on_up != 0,
        })
        .collect())
}
