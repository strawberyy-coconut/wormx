mod app;

use wormx::{Db, SqliteDialect};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = <sqlx_sqlite::SqlitePool>::connect(env!("DATABASE_URL")).await?;
    app::run(Db::new(pool, SqliteDialect)).await
}
