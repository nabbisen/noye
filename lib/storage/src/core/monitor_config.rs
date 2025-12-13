use sqlx::SqlitePool;

#[derive(Clone)]
pub struct MonitorConfig {
    pub host: String,
    pub port: i64,
    pub protocol: String,
}

pub async fn select_all(pool: &SqlitePool) -> Result<Vec<MonitorConfig>, sqlx::Error> {
    sqlx::query_as!(
        MonitorConfig,
        "
SELECT host, port, protocol
FROM monitor_config
ORDER BY host
        "
    )
    .fetch_all(pool) // -> Vec<{ country: String, count: i64 }>
    .await
}
