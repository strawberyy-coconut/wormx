use sqlx_core::connection::Connection;
use sqlx_core::query::query;
use sqlx_core::sql_str::AssertSqlSafe;
use sqlx_mysql::MySqlConnection;

const DEFAULT_URL: &str = "mysql://root:Password1!@mysql:3306/wormx_example";

#[tokio::main(flavor = "current_thread")]
async fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WORMX_REPO_CACHE");
    println!("cargo:rerun-if-env-changed=MYSQL_URL");

    let url = std::env::var("MYSQL_URL").unwrap_or_else(|_| DEFAULT_URL.to_owned());
    let mut connection = MySqlConnection::connect(&url)
        .await
        .expect("connect to the example MySQL database");

    for statement in [
        "DROP TABLE IF EXISTS app_todos",
        "DROP TABLE IF EXISTS app_users",
        r#"
        CREATE TABLE app_users (
            id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            name VARCHAR(200) NOT NULL,
            email VARCHAR(200) NULL,
            active BOOLEAN NOT NULL DEFAULT TRUE
        )
        "#,
        r#"
        CREATE TABLE app_todos (
            id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
            user_id BIGINT NOT NULL,
            title VARCHAR(200) NOT NULL,
            CONSTRAINT fk_app_todos_users FOREIGN KEY (user_id) REFERENCES app_users(id)
        )
        "#,
        "INSERT INTO app_users (name, email, active) VALUES ('Ada', 'ada@example.com', TRUE)",
        "INSERT INTO app_todos (user_id, title) VALUES (1, 'Design the schema'), (1, 'Query it with wormx')",
    ] {
        query(AssertSqlSafe(statement))
            .execute(&mut connection)
            .await
            .expect("create example schema");
    }

    let _ = connection.close().await;

    let out_dir = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let cache_dir = out_dir.join("wormx-cache");
    let _ = std::fs::remove_dir_all(&cache_dir);

    println!("cargo:rustc-env=DATABASE_URL={url}");
    let repo_cache = std::env::var("WORMX_REPO_CACHE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !repo_cache {
        println!("cargo:rustc-env=WORMX_OFFLINE_DIR={}", cache_dir.display());
    }
}
