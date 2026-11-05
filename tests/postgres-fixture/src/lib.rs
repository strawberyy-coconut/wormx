wormx::table!("public.wormx_users");
wormx::table!("public.wormx_todos");
wormx::table!("public.wormx_reviews");

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "wormx_mood", rename_all = "lowercase")]
pub enum Mood {
    Happy,
    Sad,
}

pub fn users_table() -> &'static wormx::TableDef {
    <WormxUsers as wormx::Table>::DEF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_generated() {
        let table = users_table();
        assert_eq!(table.name, "wormx_users");
        assert_eq!(table.schema, Some("public"));
        assert_eq!(table.primary_key, &["id"]);

        let id = table
            .columns
            .iter()
            .find(|column| column.name == "id")
            .unwrap();
        assert_eq!(id.sql_type, "INT8");
        assert!(!id.nullable);
        assert!(id.primary_key);
        assert!(id.has_default);

        let email = table
            .columns
            .iter()
            .find(|column| column.name == "email")
            .unwrap();
        assert!(email.nullable);

        let name_upper = table
            .columns
            .iter()
            .find(|column| column.name == "name_upper")
            .unwrap();
        assert!(name_upper.generated);
    }

    #[tokio::test]
    async fn decodes_rows() {
        let pool = sqlx::PgPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to Postgres fixture database");

        let user: WormxUsers = sqlx::query_as("SELECT * FROM public.wormx_users")
            .fetch_one(&pool)
            .await
            .expect("fetch Postgres fixture user");

        assert_eq!(user.id, 1);
        assert_eq!(user.name, "Alice");
        assert_eq!(user.email, None);
        assert!(user.active);
        assert_eq!(user.mood, Mood::Happy);
        assert_eq!(
            user.metadata,
            Some(serde_json::json!({"source": "fixture"}))
        );
    }

    #[tokio::test]
    async fn runs_typed_queries() {
        let pool = sqlx::PgPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to Postgres fixture database");
        let db = wormx::Db::new(pool, wormx::PostgresDialect);

        let users = db
            .query(WormxUsers::all())
            .filter(WormxUsers::fields().active().eq(true))
            .order_by(WormxUsers::fields().name().asc())
            .exec()
            .await
            .expect("run typed Postgres query");

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Alice");
        assert_eq!(users[0].mood, Mood::Happy);

        let user = db
            .query(WormxUsers::get(1))
            .first()
            .await
            .expect("run primary key lookup")
            .expect("user exists");
        assert_eq!(user.name, "Alice");

        let todos = db
            .query(user.wormx_todos())
            .exec()
            .await
            .expect("load has-many relation");
        assert_eq!(todos.len(), 2);

        let owner = db
            .query(todos[0].user())
            .first()
            .await
            .expect("load belongs-to relation")
            .expect("owner exists");
        assert_eq!(owner.id, 1);
        assert_eq!(owner.name, "Alice");
    }

    #[tokio::test]
    async fn eager_loads_relations() {
        let pool = sqlx::PgPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to Postgres fixture database");
        let db = wormx::Db::new(pool, wormx::PostgresDialect);

        let users = db
            .query(WormxUsers::all())
            .include(WormxUsers::fields().wormx_todos())
            .exec()
            .await
            .expect("eager load has-many relation");

        assert_eq!(users.len(), 1);
        let todos = users[0]
            .wormx_todos()
            .loaded()
            .expect("todos relation was included");
        assert_eq!(todos.len(), 2);

        let todos = db
            .query(WormxTodos::all())
            .include(WormxTodos::fields().user())
            .exec()
            .await
            .expect("eager load belongs-to relation");

        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0]
                .user()
                .loaded()
                .expect("user relation was included")
                .id,
            1
        );
    }

    #[tokio::test]
    async fn loads_multiple_incoming_foreign_keys() {
        let pool = sqlx::PgPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to Postgres fixture database");
        let db = wormx::Db::new(pool, wormx::PostgresDialect);

        let user = db
            .query(WormxUsers::all())
            .first()
            .await
            .expect("load fixture user")
            .expect("user exists");

        let authored = db
            .query(user.author_wormx_reviews())
            .exec()
            .await
            .expect("load authored reviews");
        assert_eq!(authored.len(), 1);

        let reviewed = db
            .query(user.reviewer_wormx_reviews())
            .exec()
            .await
            .expect("load reviewed reviews");
        assert_eq!(reviewed.len(), 1);

        let users = db
            .query(WormxUsers::all())
            .include(WormxUsers::fields().author_wormx_reviews())
            .exec()
            .await
            .expect("eager load multiple incoming foreign keys");
        assert_eq!(
            users[0]
                .author_wormx_reviews()
                .loaded()
                .expect("authored reviews were included")
                .len(),
            1
        );
    }
}
