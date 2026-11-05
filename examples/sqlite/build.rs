use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WORMX_REPO_CACHE");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let db_path = out_dir.join("wormx-example.db");
    let cache_dir = out_dir.join("wormx-cache");

    let _ = fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&cache_dir);

    let connection = rusqlite::Connection::open(&db_path).expect("create example database");
    connection
        .execute_batch(
            r#"
            CREATE TABLE app_users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT,
                active BOOLEAN NOT NULL DEFAULT 1
            );

            CREATE TABLE app_todos (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES app_users(id),
                title TEXT NOT NULL
            );

            INSERT INTO app_users (name, email, active)
            VALUES ('Ada', 'ada@example.com', 1);

            INSERT INTO app_todos (user_id, title)
            VALUES (1, 'Design the schema'), (1, 'Query it with wormx');
            "#,
        )
        .expect("create example schema");
    drop(connection);

    println!(
        "cargo:rustc-env=DATABASE_URL=sqlite://{}",
        db_path.display()
    );
    let repo_cache = std::env::var("WORMX_REPO_CACHE")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !repo_cache {
        println!("cargo:rustc-env=WORMX_OFFLINE_DIR={}", cache_dir.display());
    }
}
