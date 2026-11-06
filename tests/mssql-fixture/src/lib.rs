wormx::table!("dbo.wormx_users");
wormx::table!("dbo.wormx_todos");
wormx::table!("dbo.wormx_reviews");

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
        assert_eq!(table.schema, Some("dbo"));
        assert_eq!(table.primary_key, &["id"]);

        let id = table
            .columns
            .iter()
            .find(|column| column.name == "id")
            .unwrap();
        assert!(id.sql_type.eq_ignore_ascii_case("bigint"));
        assert!(!id.nullable);
        assert!(id.primary_key);
        assert!(id.identity);
    }

    #[tokio::test]
    async fn runs_typed_queries_and_relations() {
        let pool = sqlx_mssql_rs::MssqlPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to MSSQL fixture database");
        let db = wormx::Db::new(pool, wormx::MssqlDialect);

        let users = db
            .query(WormxUsers::all())
            .filter(WormxUsers::fields().active().eq(true))
            .order_by(WormxUsers::fields().id().asc())
            .exec()
            .await
            .expect("run typed MSSQL query");

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Alice");

        let todos = db
            .query(users[0].wormx_todos())
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
        assert_eq!(owner.name, "Alice");

        let ids: Vec<i64> = db
            .query(WormxTodos::all())
            .select(WormxTodos::fields().id())
            .exec()
            .await
            .expect("run projection");
        assert_eq!(ids.len(), 2);

        let count = db
            .query(WormxUsers::all())
            .count()
            .await
            .expect("run COUNT");
        assert_eq!(count, 1);

        let distinct_count = db
            .query(WormxUsers::all())
            .distinct()
            .count()
            .await
            .expect("run DISTINCT COUNT");
        assert_eq!(distinct_count, 1);

        let distinct_name_count = db
            .query(WormxUsers::all())
            .distinct()
            .select(WormxUsers::fields().name())
            .count()
            .await
            .expect("run DISTINCT projection COUNT");
        assert_eq!(distinct_name_count, 1);
    }

    #[tokio::test]
    async fn eager_loads_relations() {
        let pool = sqlx_mssql_rs::MssqlPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to MSSQL fixture database");
        let db = wormx::Db::new(pool, wormx::MssqlDialect);

        let users = db
            .query(WormxUsers::all())
            .include(WormxUsers::fields().wormx_todos())
            .exec()
            .await
            .expect("eager load has-many relation");

        assert_eq!(users[0].wormx_todos().loaded().expect("included").len(), 2);

        let todos = db
            .query(WormxTodos::all())
            .include(WormxTodos::fields().user())
            .exec()
            .await
            .expect("eager load belongs-to relation");

        assert_eq!(todos[0].user().loaded().expect("included").name, "Alice");
    }

    #[tokio::test]
    async fn loads_multiple_incoming_foreign_keys() {
        let pool = sqlx_mssql_rs::MssqlPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to MSSQL fixture database");
        let db = wormx::Db::new(pool, wormx::MssqlDialect);

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

    #[tokio::test]
    async fn returns_rows_from_dml() {
        let pool = sqlx_mssql_rs::MssqlPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to MSSQL fixture database");
        let db = wormx::Db::new(pool, wormx::MssqlDialect);
        let mut tx = db.begin().await.expect("begin transaction");

        let inserted = db
            .insert::<WormxUsers>()
            .value(WormxUsers::fields().name(), "Returning User".to_owned())
            .value(WormxUsers::fields().email(), None::<String>)
            .value(WormxUsers::fields().active(), true)
            .exec_returning_in(&mut tx)
            .await
            .expect("insert with OUTPUT");
        assert!(inserted.id > 0);
        assert_eq!(inserted.name, "Returning User");

        let updated = db
            .update::<WormxUsers>()
            .set(WormxUsers::fields().name(), "Updated User".to_owned())
            .filter(WormxUsers::fields().id().eq(inserted.id))
            .exec_returning_in(&mut tx)
            .await
            .expect("update with OUTPUT");
        assert_eq!(updated.len(), 1);
        assert_eq!(updated[0].name, "Updated User");

        let deleted = db
            .delete::<WormxUsers>()
            .filter(WormxUsers::fields().id().eq(inserted.id))
            .exec_returning_in(&mut tx)
            .await
            .expect("delete with OUTPUT");
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].name, "Updated User");

        tx.rollback().await.expect("roll back transaction");
    }

    #[tokio::test]
    async fn batch_writes_rows() {
        let pool = sqlx_mssql_rs::MssqlPool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to MSSQL fixture database");
        let db = wormx::Db::new(pool, wormx::MssqlDialect);
        let mut tx = db.begin().await.expect("begin transaction");

        let inserted = db
            .insert_many::<WormxUsers>()
            .row(|row| {
                row.value(WormxUsers::fields().name(), "Batch Alice".to_owned())
                    .value(WormxUsers::fields().email(), None::<String>)
                    .value(WormxUsers::fields().active(), true)
            })
            .row(|row| {
                row.value(WormxUsers::fields().name(), "Batch Bob".to_owned())
                    .value(WormxUsers::fields().email(), None::<String>)
                    .value(WormxUsers::fields().active(), false)
            })
            .exec_returning_in(&mut tx)
            .await
            .expect("batch insert with OUTPUT");
        assert_eq!(inserted.len(), 2);
        let ids = inserted.iter().map(|user| user.id).collect::<Vec<_>>();

        let upserted = db
            .upsert_many::<WormxUsers>()
            .row(|row| {
                row.value(WormxUsers::fields().id(), ids[0])
                    .value(WormxUsers::fields().name(), "Batch Alice 2".to_owned())
                    .value(WormxUsers::fields().email(), None::<String>)
                    .value(WormxUsers::fields().active(), true)
            })
            .row(|row| {
                row.value(WormxUsers::fields().id(), ids[1])
                    .value(WormxUsers::fields().name(), "Batch Bob 2".to_owned())
                    .value(WormxUsers::fields().email(), None::<String>)
                    .value(WormxUsers::fields().active(), true)
            })
            .row(|row| {
                row.value(WormxUsers::fields().id(), 999_999_i64)
                    .value(WormxUsers::fields().name(), "Batch Carol".to_owned())
                    .value(WormxUsers::fields().email(), None::<String>)
                    .value(WormxUsers::fields().active(), true)
            })
            .conflict_on(WormxUsers::fields().id())
            .update_from_input(WormxUsers::fields().name())
            .update_from_input(WormxUsers::fields().active())
            .exec_returning_in(&mut tx)
            .await
            .expect("batch merge with OUTPUT");
        assert_eq!(upserted.len(), 3);
        let carol = upserted
            .iter()
            .find(|user| user.name == "Batch Carol")
            .expect("inserted merge row");
        assert_ne!(carol.id, 999_999, "identity value is generated");

        db.update_many::<WormxUsers>()
            .key(WormxUsers::fields().id())
            .row(|row| {
                row.key(ids[0])
                    .value(WormxUsers::fields().name(), "Batch Alice 3".to_owned())
            })
            .row(|row| {
                row.key(ids[1])
                    .value(WormxUsers::fields().name(), "Batch Bob 3".to_owned())
            })
            .exec_in(&mut tx)
            .await
            .expect("batch update");

        let users = db
            .query(WormxUsers::all())
            .filter(WormxUsers::fields().id().in_list(ids.clone()))
            .order_by(WormxUsers::fields().id().asc())
            .exec_in(&mut tx)
            .await
            .expect("load batch rows");
        assert_eq!(users.len(), 2);
        assert_eq!(users[0].name, "Batch Alice 3");
        assert_eq!(users[1].name, "Batch Bob 3");

        let deleted = db
            .delete::<WormxUsers>()
            .filter(WormxUsers::fields().name().like("Batch%".to_owned()))
            .exec_returning_in(&mut tx)
            .await
            .expect("batch delete with OUTPUT");
        assert_eq!(deleted.len(), 3);

        tx.rollback().await.expect("roll back transaction");
    }
}
