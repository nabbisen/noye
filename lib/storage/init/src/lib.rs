use migration::MigratorTrait;
use noye_const::DATABASE_PATH;
use sea_orm::{ConnectOptions, Database};

pub async fn init_db() -> () {
    let connect_options = ConnectOptions::new(format!("{}?mode=rwc", DATABASE_PATH)).to_owned();

    let db = Database::connect(connect_options).await.unwrap();

    migration::Migrator::up(&db, None).await.unwrap();

    println!("database initialized");
}
