mod app;

use wormx::{Db, MssqlDialect};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = <sqlx_mssql_rs::MssqlPool>::connect(env!("DATABASE_URL")).await?;
    app::run(Db::new(pool, MssqlDialect)).await
}
