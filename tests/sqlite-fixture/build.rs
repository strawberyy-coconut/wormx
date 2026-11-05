use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let db_path = out_dir.join("wormx-fixture.db");
    let cache_dir = out_dir.join("wormx-cache");

    let _ = fs::remove_file(&db_path);
    let _ = fs::remove_dir_all(&cache_dir);

    let connection = rusqlite::Connection::open(&db_path).expect("create fixture database");
    connection
        .execute_batch(
            r#"
            CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT,
                active BOOLEAN NOT NULL DEFAULT 1
            );

            CREATE TABLE todos (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id),
                reviewer_id INTEGER REFERENCES users(id),
                title TEXT NOT NULL
            );

            CREATE TABLE accounts (
                tenant TEXT NOT NULL,
                id INTEGER NOT NULL,
                name TEXT NOT NULL,
                PRIMARY KEY (tenant, id)
            );

            CREATE TABLE orders (
                id INTEGER PRIMARY KEY,
                tenant TEXT NOT NULL,
                account_id INTEGER NOT NULL,
                total INTEGER NOT NULL,
                FOREIGN KEY (tenant, account_id) REFERENCES accounts(tenant, id)
            );

            CREATE TABLE shipments (
                id INTEGER PRIMARY KEY,
                tenant TEXT,
                account_id INTEGER,
                label TEXT NOT NULL,
                FOREIGN KEY (tenant, account_id) REFERENCES accounts(tenant, id)
            );

            CREATE TABLE teams (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL
            );

            CREATE TABLE user_teams (
                user_id INTEGER NOT NULL REFERENCES users(id),
                team_id INTEGER NOT NULL REFERENCES teams(id),
                PRIMARY KEY (user_id, team_id)
            );

            INSERT INTO users (id, name, email, active)
            VALUES (1, 'Alice', NULL, 1);

            INSERT INTO todos (id, user_id, title)
            VALUES (1, 1, 'Write the ORM'), (2, 1, 'Add relations');

            INSERT INTO accounts (tenant, id, name)
            VALUES ('acme', 1, 'Acme'), ('acme', 2, 'Acme Two'), ('other', 1, 'Other');

            INSERT INTO orders (id, tenant, account_id, total)
            VALUES (1, 'acme', 1, 10), (2, 'acme', 1, 20), (3, 'other', 1, 30);

            INSERT INTO shipments (id, tenant, account_id, label)
            VALUES (1, 'acme', 1, 'first'), (2, NULL, NULL, 'unassigned');

            INSERT INTO teams (id, name) VALUES (1, 'Core'), (2, 'Docs');

            INSERT INTO user_teams (user_id, team_id) VALUES (1, 1), (1, 2);
            "#,
        )
        .expect("create fixture schema");
    drop(connection);

    let database_url = format!("sqlite://{}", db_path.display());
    println!("cargo:rustc-env=DATABASE_URL={database_url}");
    println!("cargo:rustc-env=WORMX_OFFLINE_DIR={}", cache_dir.display());
}
