use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=POSTGRES_URL");

    let url = postgres_url();
    let mut client = postgres::Client::connect(&url, postgres::NoTls)
        .expect("connect to Postgres fixture database");

    client
        .batch_execute(
            r#"
            DROP TABLE IF EXISTS public.wormx_reviews CASCADE;
            DROP TABLE IF EXISTS public.wormx_todos CASCADE;
            DROP TABLE IF EXISTS public.wormx_users CASCADE;
            DROP TYPE IF EXISTS public.wormx_mood CASCADE;

            CREATE TYPE public.wormx_mood AS ENUM ('happy', 'sad');

            CREATE TABLE public.wormx_users (
                id BIGSERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT,
                active BOOLEAN NOT NULL DEFAULT true,
                mood public.wormx_mood NOT NULL DEFAULT 'happy',
                metadata JSONB,
                created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
                name_upper TEXT GENERATED ALWAYS AS (upper(name)) STORED
            );

            INSERT INTO public.wormx_users (name, email, active, metadata, mood)
            VALUES ('Alice', NULL, true, '{"source":"fixture"}', 'happy');

            CREATE TABLE public.wormx_todos (
                id BIGSERIAL PRIMARY KEY,
                user_id BIGINT NOT NULL REFERENCES public.wormx_users(id),
                title TEXT NOT NULL
            );

            INSERT INTO public.wormx_todos (user_id, title)
            VALUES (1, 'Write the ORM'), (1, 'Add relations');

            CREATE TABLE public.wormx_reviews (
                id BIGSERIAL PRIMARY KEY,
                author_id BIGINT NOT NULL REFERENCES public.wormx_users(id),
                reviewer_id BIGINT NOT NULL REFERENCES public.wormx_users(id),
                body TEXT NOT NULL
            );

            INSERT INTO public.wormx_reviews (author_id, reviewer_id, body)
            VALUES (1, 1, 'Looks good');
            "#,
        )
        .expect("create Postgres fixture schema");

    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    let cache_dir = out_dir.join("wormx-cache");
    let _ = std::fs::remove_dir_all(&cache_dir);

    println!("cargo:rustc-env=DATABASE_URL={url}");
    println!("cargo:rustc-env=WORMX_OFFLINE_DIR={}", cache_dir.display());
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
