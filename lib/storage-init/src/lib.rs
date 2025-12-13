use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

pub async fn init_db() -> () {
    dotenvy::dotenv().ok();
    let database_url = &std::env::var("DATABASE_URL").expect("DATABASE_URL is missing in .env");

    let options = SqliteConnectOptions::from_str(database_url)
        .unwrap()
        .create_if_missing(true);

    let conn = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();

    sqlx::migrate!("./migrations").run(&conn).await.unwrap();

    println!("database initialized");
}
