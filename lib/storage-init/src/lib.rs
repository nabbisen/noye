use noye_const::DATABASE_PATH;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

pub async fn init_db() -> () {
    let options = SqliteConnectOptions::from_str(DATABASE_PATH)
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
