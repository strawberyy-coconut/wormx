use sqlx_core::connection::Connection;
use sqlx_core::query::query;
use sqlx_mssql_rs::MssqlConnection;

const DEFAULT_URL: &str =
    "mssql://sa:Password1!@mssql:1433/master?encrypt=on&trust_certificate=true";
const DATABASE: &str = "wormx_example";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WORMX_REPO_CACHE");
    println!("cargo:rerun-if-env-changed=MSSQL_URL");

    let base = std::env::var("MSSQL_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned());
    let master_url = with_database(&base, "master");
    let target_url = with_database(&base, DATABASE);

    let mut master = MssqlConnection::connect(&master_url)
        .await
        .expect("connect to MSSQL master");
    query(sqlx_core::sql_str::AssertSqlSafe(format!(
        "IF DB_ID(N'{DATABASE}') IS NULL CREATE DATABASE [{DATABASE}]"
    )))
    .execute(&mut master)
    .await
    .expect("create example database");
    let _ = master.close().await;

    let mut connection = MssqlConnection::connect(&target_url)
        .await
        .expect("connect to example database");

    for statement in [
        "IF OBJECT_ID(N'dbo.app_todos', N'U') IS NOT NULL DROP TABLE dbo.app_todos",
        "IF OBJECT_ID(N'dbo.app_users', N'U') IS NOT NULL DROP TABLE dbo.app_users",
        r#"
        CREATE TABLE dbo.app_users (
            id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
            name NVARCHAR(200) NOT NULL,
            email NVARCHAR(200) NULL,
            active BIT NOT NULL CONSTRAINT DF_app_users_active DEFAULT 1
        )
        "#,
        r#"
        CREATE TABLE dbo.app_todos (
            id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
            user_id BIGINT NOT NULL,
            title NVARCHAR(200) NOT NULL,
            CONSTRAINT FK_app_todos_users FOREIGN KEY (user_id) REFERENCES dbo.app_users(id)
        )
        "#,
        "INSERT INTO dbo.app_users (name, email, active) VALUES (N'Ada', N'ada@example.com', 1)",
        "INSERT INTO dbo.app_todos (user_id, title) VALUES (1, N'Design the schema'), (1, N'Query it with wormx')",
    ] {
        query(sqlx_core::sql_str::AssertSqlSafe(statement))
            .execute(&mut connection)
            .await
            .expect("create example schema");
    }

    let _ = connection.close().await;

    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let cache_dir = out_dir.join("wormx-cache");
    let _ = std::fs::remove_dir_all(&cache_dir);

    println!("cargo:rustc-env=DATABASE_URL={target_url}");
    let repo_cache = std::env::var("WORMX_REPO_CACHE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !repo_cache {
        println!("cargo:rustc-env=WORMX_OFFLINE_DIR={}", cache_dir.display());
    }
}

fn with_database(url: &str, database: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let prefix = base
        .rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .unwrap_or(base);
    match query {
        Some(query) => format!("{prefix}/{database}?{query}"),
        None => format!("{prefix}/{database}"),
    }
}
