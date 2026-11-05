use sqlx_core::connection::Connection;
use sqlx_core::query::query;
use sqlx_mssql_rs::MssqlConnection;

const DEFAULT_URL: &str =
    "mssql://sa:Password1!@mssql:1433/master?encrypt=on&trust_certificate=true";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MSSQL_URL");

    let base = std::env::var("MSSQL_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned());
    let master_url = with_database(&base, "master");
    let target_url = with_database(&base, "wormx_test");

    let mut master = MssqlConnection::connect(&master_url)
        .await
        .expect("connect to MSSQL master");
    query("IF DB_ID(N'wormx_test') IS NULL CREATE DATABASE [wormx_test]")
        .execute(&mut master)
        .await
        .expect("create wormx_test database");
    let _ = master.close().await;

    let mut connection = MssqlConnection::connect(&target_url)
        .await
        .expect("connect to wormx_test");

    for statement in [
        "IF OBJECT_ID(N'dbo.wormx_reviews', N'U') IS NOT NULL DROP TABLE dbo.wormx_reviews",
        "IF OBJECT_ID(N'dbo.wormx_todos', N'U') IS NOT NULL DROP TABLE dbo.wormx_todos",
        "IF OBJECT_ID(N'dbo.wormx_users', N'U') IS NOT NULL DROP TABLE dbo.wormx_users",
        r#"
        CREATE TABLE dbo.wormx_users (
            id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
            name NVARCHAR(200) NOT NULL,
            email NVARCHAR(200) NULL,
            active BIT NOT NULL CONSTRAINT DF_wormx_users_active DEFAULT 1
        )
        "#,
        r#"
        CREATE TABLE dbo.wormx_todos (
            id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
            user_id BIGINT NOT NULL,
            title NVARCHAR(200) NOT NULL,
            CONSTRAINT FK_wormx_todos_users
                FOREIGN KEY (user_id) REFERENCES dbo.wormx_users(id)
        )
        "#,
        r#"
        CREATE TABLE dbo.wormx_reviews (
            id BIGINT IDENTITY(1,1) NOT NULL PRIMARY KEY,
            author_id BIGINT NOT NULL,
            reviewer_id BIGINT NOT NULL,
            body NVARCHAR(200) NOT NULL,
            CONSTRAINT FK_wormx_reviews_author
                FOREIGN KEY (author_id) REFERENCES dbo.wormx_users(id),
            CONSTRAINT FK_wormx_reviews_reviewer
                FOREIGN KEY (reviewer_id) REFERENCES dbo.wormx_users(id)
        )
        "#,
        "INSERT INTO dbo.wormx_users (name, email, active) VALUES (N'Alice', NULL, 1)",
        "INSERT INTO dbo.wormx_todos (user_id, title) VALUES (1, N'Write the ORM'), (1, N'Add relations')",
        "INSERT INTO dbo.wormx_reviews (author_id, reviewer_id, body) VALUES (1, 1, N'Looks good')",
    ] {
        query(sqlx_core::sql_str::AssertSqlSafe(statement))
            .execute(&mut connection)
            .await
            .expect("create MSSQL fixture schema");
    }

    let _ = connection.close().await;

    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let cache_dir = out_dir.join("wormx-cache");
    let _ = std::fs::remove_dir_all(&cache_dir);

    println!("cargo:rustc-env=DATABASE_URL={target_url}");
    println!("cargo:rustc-env=WORMX_OFFLINE_DIR={}", cache_dir.display());
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
