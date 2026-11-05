use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=WORMX_REPO_CACHE");
    println!("cargo:rerun-if-env-changed=POSTGRES_URL");

    let url = postgres_url();
    let mut client = postgres::Client::connect(&url, postgres::NoTls)
        .expect("connect to the example Postgres database");

    client
        .batch_execute(
            r#"
            DROP TABLE IF EXISTS public.app_todos CASCADE;
            DROP TABLE IF EXISTS public.app_users CASCADE;

            CREATE TABLE public.app_users (
                id BIGSERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT,
                active BOOLEAN NOT NULL DEFAULT true
            );

            CREATE TABLE public.app_todos (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES public.app_users(id),
                title TEXT NOT NULL
            );

            INSERT INTO public.app_users (name, email, active)
            VALUES ('Ada', 'ada@example.com', true);

            INSERT INTO public.app_todos (user_id, title)
            VALUES (1, 'Design the schema'), (1, 'Query it with wormx');
            "#,
        )
        .expect("create example schema");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
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

fn postgres_url() -> String {
    if let Ok(url) = std::env::var("POSTGRES_URL") {
        return url;
    }

    if let Ok(url) = std::env::var("DATABASE_URL")
        && (url.starts_with("postgres://") || url.starts_with("postgresql://"))
    {
        return url;
    }

    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let contents = std::fs::read_to_string(root.join(".dev.env")).unwrap_or_default();
    for line in contents.lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("DATABASE_URL=")
            && (value.starts_with("postgres://") || value.starts_with("postgresql://"))
        {
            return value.to_owned();
        }
    }

    panic!("set POSTGRES_URL or DATABASE_URL to a Postgres database");
}
