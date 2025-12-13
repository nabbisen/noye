use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

pub async fn connect() -> Result<SqlitePool, sqlx::Error> {
    dotenvy::dotenv().ok();
    let database_url = &std::env::var("DATABASE_URL").expect("DATABASE_URL is missing in .env");
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect(database_url)
        .await
}
