use noye_const::DATABASE_PATH;
use sea_orm::{ConnectOptions, Database, DatabaseConnection, DbErr};

pub async fn db_conn() -> Result<DatabaseConnection, DbErr> {
    let opt = ConnectOptions::new(DATABASE_PATH);
    Database::connect(opt).await
}
