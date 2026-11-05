mod app;

use wormx::{Db, PostgresDialect};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = <sqlx_postgres::PgPool>::connect(env!("DATABASE_URL")).await?;
    app::run(Db::new(pool, PostgresDialect)).await
}
