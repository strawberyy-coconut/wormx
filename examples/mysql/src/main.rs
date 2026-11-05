mod app;

use wormx::{Db, MySqlDialect};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = <sqlx_mysql::MySqlPool>::connect(env!("DATABASE_URL")).await?;
    app::run(Db::new(pool, MySqlDialect)).await
}
