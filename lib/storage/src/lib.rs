use noye_const::DATABASE_PATH;
use sqlx::{SqlitePool, sqlite::SqlitePoolOptions};

pub async fn db_conn() -> Result<SqlitePool, sqlx::Error> {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect(DATABASE_PATH)
        .await
}
