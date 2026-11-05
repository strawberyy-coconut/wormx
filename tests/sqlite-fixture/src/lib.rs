wormx::table!("users");
wormx::table!("todos");
wormx::table!("accounts");
wormx::table!("orders");
wormx::table!("shipments");
wormx::table!("teams");
wormx::table!("user_teams");

wormx::sql_function! {
    pub fn lower(value: String) -> String;
}

wormx::operator! {
    pub fn concat(left: String, right: String) -> String = "||";
    fixity = Infix;
    grouping = Operands;
}

wormx::operator! {
    pub fn not_(value: bool) -> bool = "NOT";
    fixity = Prefix;
    grouping = Operands;
}

wormx::operator! {
    pub fn raw_concat(left: String, right: String) -> String = "||";
    fixity = Infix;
    grouping = None;
}

wormx::operator! {
    pub fn explicit_not(value: bool) -> bool = "NOT";
    fixity = Prefix;
    grouping = Operands;
}

wormx::operator! {
    pub fn prec_concat(left: String, right: String) -> String = "||";
    fixity = Infix;
    grouping = Precedence(6);
}

pub fn users_table() -> &'static wormx::TableDef {
    <Users as wormx::Table>::DEF
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_is_generated() {
        let table = users_table();
        assert_eq!(table.name, "users");
        assert_eq!(table.schema, None);
        assert_eq!(table.primary_key, &["id"]);

        let id = table
            .columns
            .iter()
            .find(|column| column.name == "id")
            .expect("id column");
        assert_eq!(id.sql_type, "INTEGER");
        assert!(!id.nullable);
        assert!(id.primary_key);

        let email = table
            .columns
            .iter()
            .find(|column| column.name == "email")
            .expect("email column");
        assert!(email.nullable);
    }

    #[test]
    fn renders_dialect_functions() {
        use wormx::{Expr, Function, SqlWriter, SqliteDialect};

        struct Upper;

        impl Function<SqliteDialect> for Upper {
            type Args = (Expr<String, SqliteDialect>,);
            type Output = String;
            const NAME: &'static str = "UPPER";
        }

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(&dialect);
        let mut expr = wormx::call::<SqliteDialect, Upper>((Expr::column("users", "name"),));

        expr.render(&mut writer).expect("render expression");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "UPPER(\"users\".\"name\")");

        let mut writer = SqlWriter::new(&dialect);
        let mut expr = concat(Users::fields().name(), "!");
        expr.render(&mut writer).expect("render operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(\"users\".\"name\") || (?)");

        let mut writer = SqlWriter::new(&dialect);
        let mut expr = Users::fields()
            .active()
            .eq(true)
            .or(Users::fields().active().eq(false))
            .and(Users::fields().name().eq("Alice"));
        expr.render(&mut writer).expect("render boolean expression");
        let (sql, _args) = writer.finish();
        assert!(sql.contains(") AND ("), "{sql}");
    }

    #[test]
    fn custom_prefix_operator_groups_its_operand() {
        use wormx::{Expr, SqlWriter, SqliteDialect};

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(&dialect);
        let mut expr = not_(Expr::<bool, SqliteDialect>::raw("a").and(Expr::raw("b")));

        expr.render(&mut writer).expect("render prefix operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "NOT ((a) AND (b))");

        let mut writer = SqlWriter::new(&dialect);
        let mut expr = explicit_not(Expr::<bool, SqliteDialect>::raw("a"));
        expr.render(&mut writer).expect("render operator! prefix");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "NOT (a)");

        let mut writer = SqlWriter::new(&dialect);
        let mut expr = raw_concat(Expr::<String, SqliteDialect>::raw("a"), Expr::raw("b"));
        expr.render(&mut writer).expect("render ungrouped operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "a || b");

        let mut writer = SqlWriter::new(&dialect);
        let mut expr = prec_concat(Expr::<String, SqliteDialect>::raw("a"), Expr::raw("b"));
        expr.render(&mut writer)
            .expect("render precedence operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(a) || (b)");
    }

    #[test]
    fn nested_infix_operators_are_grouped_and_expr_group_is_explicit() {
        use wormx::{Expr, Fixity, Operator, OperatorExpr, SqlWriter, SqliteDialect};

        struct Concat;

        impl Operator<SqliteDialect> for Concat {
            type Left = Expr<String, SqliteDialect>;
            type Right = Expr<String, SqliteDialect>;
            type Output = String;
            const SQL: &'static str = "||";
            const FIXITY: Fixity = Fixity::Infix;
        }

        let dialect = SqliteDialect;
        let inner =
            Expr::<i64, SqliteDialect>::infix::<i64, i64>(Expr::raw("b"), "*", Expr::raw("c"));
        let mut expr = Expr::<i64, SqliteDialect>::infix::<i64, i64>(Expr::raw("a"), "+", inner);

        let mut writer = SqlWriter::new(&dialect);
        expr.render(&mut writer).expect("render nested operators");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(a) + ((b) * (c))");

        let mut writer = SqlWriter::new(&dialect);
        let mut grouped = Expr::<i64, SqliteDialect>::raw("a + b").group();
        grouped.render(&mut writer).expect("render grouped expr");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(a + b)");

        let mut writer = SqlWriter::new(&dialect);
        let mut postfix = Expr::<i64, SqliteDialect>::postfix(Expr::raw("x"), "::int");
        postfix
            .render(&mut writer)
            .expect("render postfix operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(x) ::int");

        let mut writer = SqlWriter::new(&dialect);
        let mut operator = Expr::<String, SqliteDialect>::new(
            OperatorExpr::<Concat, SqliteDialect>::new(Expr::raw("a"), Expr::raw("b")),
        );
        operator
            .render(&mut writer)
            .expect("render custom operator expression");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(a) || (b)");
    }

    #[test]
    fn external_renderer_can_render_joins() {
        use wormx::{
            Expr, SelectPlan, SelectRenderer, SqlSelect, SqlWriter, SqliteDialect, Table,
            TableSource,
        };

        #[derive(Default)]
        struct JoinSelect;

        impl SelectRenderer<SqliteDialect> for JoinSelect {
            fn write<P>(
                &self,
                plan: SelectPlan<'_, SqliteDialect, P>,
                writer: &mut SqlWriter<'_, SqliteDialect>,
            ) -> sqlx::Result<()>
            where
                P: wormx::ProjectionWriter<SqliteDialect>,
            {
                writer.write_select(plan.distinct);
                plan.projection.write(writer)?;
                wormx::write_from_filters(writer, plan.table, plan.filters, plan.joins)?;
                wormx::write_groups_having(writer, plan.groups, plan.having)?;
                wormx::write_orders(writer, plan.orders)?;
                writer.push_limit_offset(!plan.orders.is_empty(), plan.limit, plan.offset);
                wormx::write_tail(writer, plan.tail);
                Ok(())
            }

            fn write_count<P>(
                &self,
                plan: SelectPlan<'_, SqliteDialect, P>,
                writer: &mut SqlWriter<'_, SqliteDialect>,
                distinct: bool,
            ) -> sqlx::Result<()>
            where
                P: wormx::ProjectionWriter<SqliteDialect> + wormx::CountProjection<SqliteDialect>,
            {
                SqlSelect.write_count(plan, writer, distinct)
            }

            fn write_count_star(
                &self,
                plan: wormx::CountPlan<'_, SqliteDialect>,
                writer: &mut SqlWriter<'_, SqliteDialect>,
            ) -> sqlx::Result<()> {
                SqlSelect.write_count_star(plan, writer)
            }
        }

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(&dialect);
        let mut projection = wormx::StarProjection;
        let mut filters = vec![Expr::<bool, SqliteDialect>::raw("1 = 1")];
        let mut joins = vec![wormx::Join::inner(
            Todos::alias("t").table(),
            Users::fields().id().eq(Todos::alias("t").user_id()),
        )];
        let mut orders = Vec::new();
        let mut groups = Vec::new();
        let mut having = Vec::new();

        JoinSelect
            .write(
                SelectPlan {
                    table: TableSource::new(Users::DEF, None).with_suffix("ROWLOCK"),
                    distinct: false,
                    projection: &mut projection,
                    filters: &mut filters,
                    joins: &mut joins,
                    groups: &mut groups,
                    having: &mut having,
                    orders: &mut orders,
                    limit: None,
                    offset: None,
                    tail: Some("RECOMPILE"),
                },
                &mut writer,
            )
            .expect("render external select renderer");

        let (sql, _args) = writer.finish();
        assert!(sql.contains(" JOIN "), "{sql}");
        assert!(sql.contains("\"t\""), "{sql}");
        assert!(!sql.contains("ROWLOCK"), "{sql}");
        assert!(!sql.contains("OPTION"), "{sql}");
    }

    #[tokio::test]
    async fn decodes_rows() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");

        let user: Users = sqlx::query_as("SELECT id, name, email, active FROM users")
            .fetch_one(&pool)
            .await
            .expect("fetch fixture user");

        assert_eq!(user.id, 1);
        assert_eq!(user.name, "Alice");
        assert_eq!(user.email, None);
        assert!(user.active);
    }

    #[tokio::test]
    async fn runs_typed_queries() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let users = db
            .query(Users::all())
            .filter(Users::fields().active().eq(true))
            .filter(Users::fields().id().eq(1_i64))
            .order_by(Users::fields().name().asc())
            .exec()
            .await
            .expect("run typed query");

        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Alice");

        let user = db
            .query(Users::get(1))
            .first()
            .await
            .expect("run primary key lookup")
            .expect("user exists");
        assert_eq!(user.name, "Alice");

        let constructed = Users::new(
            2,
            "Bob".to_owned(),
            Some("bob@example.com".to_owned()),
            false,
        );
        assert_eq!(constructed.name, "Bob");

        let todos = db
            .query(user.user_todos())
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

        let lowered = db
            .query(Users::all())
            .filter(
                lower(Users::fields().name())
                    .eq("alice")
                    .and(Users::fields().active().eq(true)),
            )
            .exec()
            .await
            .expect("run custom SQL function and AND");
        assert_eq!(lowered.len(), 1);

        let coalesced = db
            .query(Users::all())
            .filter(wormx::coalesce(Users::fields().email(), "none").eq("none"))
            .filter(Users::fields().id().eq(1_i64))
            .exec()
            .await
            .expect("run COALESCE expression");
        assert_eq!(coalesced.len(), 1);

        let negated = db
            .query(Users::all())
            .filter(Users::fields().active().eq(false).not())
            .filter(Users::fields().id().eq(1_i64))
            .exec()
            .await
            .expect("run NOT expression");
        assert_eq!(negated.len(), 1);

        let reviewer = db
            .query(todos[0].reviewer())
            .first()
            .await
            .expect("load nullable belongs-to relation");
        assert!(reviewer.is_none());

        let count = db
            .query(Users::all())
            .filter(Users::fields().active().eq(true))
            .count()
            .await
            .expect("run COUNT query");
        assert_eq!(count, 1);

        let name_count = db
            .query(Users::all())
            .select(Users::fields().name())
            .count()
            .await
            .expect("run COUNT(column)");
        assert_eq!(name_count, 1);

        let distinct_users = db
            .query(Todos::all())
            .select(Todos::fields().user_id())
            .count_distinct()
            .await
            .expect("run COUNT(DISTINCT column)");
        assert_eq!(distinct_users, 1);

        let aggregated = db
            .query(Users::all())
            .select(Users::fields().name().count())
            .first()
            .await
            .expect("run aggregate projection");
        assert_eq!(aggregated, Some(1));

        let distinct_names: Vec<String> = db
            .query(Users::all())
            .distinct()
            .select(Users::fields().name())
            .exec()
            .await
            .expect("run DISTINCT projection");
        assert_eq!(distinct_names, vec!["Alice".to_owned()]);

        let joined: Vec<(String, String)> = db
            .query(Users::all())
            .join(wormx::Join::inner(
                Todos::alias("t").table(),
                Users::fields().id().eq(Todos::alias("t").user_id()),
            ))
            .select((Users::fields().name(), Todos::alias("t").title()))
            .order_by(Todos::alias("t").id().asc())
            .exec()
            .await
            .expect("run joined query");
        assert_eq!(joined.len(), 2);
        assert_eq!(joined[0].0, "Alice");

        let grouped: Vec<(i64, i64)> = db
            .query(Todos::all())
            .select((Todos::fields().user_id(), Todos::fields().id().count()))
            .group_by(Todos::fields().user_id())
            .having(Todos::fields().id().count().gt(1_i64))
            .exec()
            .await
            .expect("run GROUP BY / HAVING");
        assert_eq!(grouped, vec![(1, 2)]);

        let raw: Vec<Users> = db
            .sql("SELECT * FROM users WHERE active = ?")
            .bind(true)
            .fetch_all()
            .await
            .expect("run raw query");
        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].name, "Alice");

        let ids: Vec<i64> = db
            .query(Todos::all())
            .select(Todos::fields().id())
            .exec()
            .await
            .expect("run single-column projection");
        assert_eq!(ids.len(), 2);

        let filtered = db
            .query(Users::filter(Users::fields().active().eq(true)))
            .exec()
            .await
            .expect("run generated filter query");
        assert_eq!(filtered.len(), 1);

        let pairs: Vec<(i64, String)> = db
            .query(Todos::all())
            .select((Todos::fields().id(), Todos::fields().title()))
            .order_by(Todos::fields().id().asc())
            .exec()
            .await
            .expect("run multi-column projection");
        assert_eq!(pairs[0], (1, "Write the ORM".to_owned()));

        let computed: Vec<(String, i64)> = db
            .query(Users::all())
            .select((lower(Users::fields().name()), Users::fields().id()))
            .exec()
            .await
            .expect("run computed projection");
        assert_eq!(computed[0], ("alice".to_owned(), 1));

        let subquery = db
            .query(Todos::all())
            .filter(Todos::fields().user_id().eq(1_i64))
            .select(Todos::fields().user_id());
        let users = db
            .query(Users::all())
            .filter(Users::fields().id().in_query(subquery))
            .exec()
            .await
            .expect("run IN subquery");
        assert_eq!(users.len(), 1);
        assert_eq!(users[0].name, "Alice");
    }

    #[tokio::test]
    async fn eager_loads_relations() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let users = db
            .query(Users::all())
            .filter(Users::fields().id().eq(1_i64))
            .include(Users::fields().user_todos())
            .exec()
            .await
            .expect("eager load has-many relation");

        assert_eq!(users.len(), 1);
        assert_eq!(
            users[0]
                .user_todos()
                .loaded()
                .expect("todos relation was included")
                .len(),
            2
        );

        let todos = db
            .query(Todos::all())
            .include(Todos::fields().user())
            .exec()
            .await
            .expect("eager load belongs-to relation");

        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0]
                .user()
                .loaded()
                .expect("user relation was included")
                .name,
            "Alice"
        );
    }

    #[tokio::test]
    async fn custom_select_renderer_is_used() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        use wormx::{Dialect, SelectRenderer, SqlSelect};

        static SELECT_CALLS: AtomicUsize = AtomicUsize::new(0);

        #[derive(Debug, Clone, Copy, Default)]
        struct TrackingDialect;

        #[derive(Default)]
        struct TrackingSelect;

        impl SelectRenderer<TrackingDialect> for TrackingSelect {
            fn write<P>(
                &self,
                plan: wormx::SelectPlan<'_, TrackingDialect, P>,
                writer: &mut wormx::SqlWriter<'_, TrackingDialect>,
            ) -> sqlx::Result<()>
            where
                P: wormx::ProjectionWriter<TrackingDialect>,
            {
                SELECT_CALLS.fetch_add(1, Ordering::SeqCst);
                SqlSelect.write(plan, writer)
            }

            fn write_count<P>(
                &self,
                plan: wormx::SelectPlan<'_, TrackingDialect, P>,
                writer: &mut wormx::SqlWriter<'_, TrackingDialect>,
                distinct: bool,
            ) -> sqlx::Result<()>
            where
                P: wormx::ProjectionWriter<TrackingDialect>
                    + wormx::CountProjection<TrackingDialect>,
            {
                SELECT_CALLS.fetch_add(1, Ordering::SeqCst);
                SqlSelect.write_count(plan, writer, distinct)
            }

            fn write_count_star(
                &self,
                plan: wormx::CountPlan<'_, TrackingDialect>,
                writer: &mut wormx::SqlWriter<'_, TrackingDialect>,
            ) -> sqlx::Result<()> {
                SELECT_CALLS.fetch_add(1, Ordering::SeqCst);
                SqlSelect.write_count_star(plan, writer)
            }
        }

        impl wormx::SqlSyntax for TrackingDialect {}

        impl Dialect for TrackingDialect {
            type DB = sqlx::Sqlite;
            type Select = TrackingSelect;
            type Insert = wormx::SqlInsert;
            type Update = wormx::SqlUpdate;
            type Delete = wormx::SqlDelete;
            type Upsert = wormx::SqlUpsert;

            fn quote_ident(&self, name: &str) -> String {
                format!("\"{}\"", name.replace('"', "\"\""))
            }
        }

        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, TrackingDialect);

        let users = db
            .query(Users::all())
            .filter(Users::fields().id().eq(1_i64))
            .exec()
            .await
            .expect("run query with custom renderer");
        assert_eq!(users.len(), 1);

        let count = db
            .query(Users::all())
            .filter(Users::fields().id().eq(1_i64))
            .count()
            .await
            .expect("run count with custom renderer");
        assert_eq!(count, 1);
        assert!(SELECT_CALLS.load(Ordering::SeqCst) >= 2);
    }

    #[tokio::test]
    async fn inserts_updates_and_deletes_rows() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        db.delete::<Users>()
            .filter(Users::fields().id().eq(40_i64))
            .exec()
            .await
            .expect("clean prior fixture row");

        db.insert::<Users>()
            .value(Users::fields().id(), 40_i64)
            .value(Users::fields().name(), "Carol")
            .value(Users::fields().email(), None::<String>)
            .value(Users::fields().active(), false)
            .exec()
            .await
            .expect("insert user");

        let carol = db
            .query(Users::get(40_i64))
            .first()
            .await
            .expect("load inserted user")
            .expect("user exists");
        assert_eq!(carol.name, "Carol");

        db.update::<Users>()
            .set(Users::fields().name(), "Caroline")
            .filter(Users::fields().id().eq(40_i64))
            .exec()
            .await
            .expect("update user");

        let updated = db
            .query(Users::get(40_i64))
            .first()
            .await
            .expect("load updated user")
            .expect("user exists");
        assert_eq!(updated.name, "Caroline");

        let returned = db
            .update::<Users>()
            .set(Users::fields().active(), true)
            .filter(Users::fields().id().eq(40_i64))
            .exec_returning()
            .await
            .expect("update with returning");
        assert_eq!(returned.len(), 1);
        assert!(returned[0].active);

        db.delete::<Users>()
            .filter(Users::fields().id().eq(40_i64))
            .exec()
            .await
            .expect("delete user");

        assert!(
            db.query(Users::get(40_i64))
                .first()
                .await
                .expect("query after delete")
                .is_none()
        );
    }

    #[tokio::test]
    async fn transactions_commit_and_roll_back() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        for id in [50_i64, 51] {
            db.delete::<Users>()
                .filter(Users::fields().id().eq(id))
                .exec()
                .await
                .expect("clean prior fixture rows");
        }

        let mut tx = db.begin().await.expect("begin transaction");
        db.insert::<Users>()
            .value(Users::fields().id(), 50_i64)
            .value(Users::fields().name(), "Rolled Back")
            .value(Users::fields().email(), None::<String>)
            .value(Users::fields().active(), true)
            .exec_in(&mut tx)
            .await
            .expect("insert inside transaction");
        tx.rollback().await.expect("roll back");

        assert!(
            db.query(Users::get(50_i64))
                .first()
                .await
                .expect("query rolled back row")
                .is_none()
        );

        let mut tx = db.begin().await.expect("begin transaction");
        db.insert::<Users>()
            .value(Users::fields().id(), 51_i64)
            .value(Users::fields().name(), "Committed")
            .value(Users::fields().email(), None::<String>)
            .value(Users::fields().active(), true)
            .exec_in(&mut tx)
            .await
            .expect("insert inside transaction");
        tx.commit().await.expect("commit");

        assert!(
            db.query(Users::get(51_i64))
                .first()
                .await
                .expect("query committed row")
                .is_some()
        );

        db.delete::<Users>()
            .filter(Users::fields().id().eq(51_i64))
            .exec()
            .await
            .expect("clean committed fixture row");
    }

    #[tokio::test]
    async fn loads_composite_foreign_keys() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let accounts = db
            .query(Accounts::all())
            .include(Accounts::fields().orders())
            .order_by(Accounts::fields().tenant().asc())
            .order_by(Accounts::fields().id().asc())
            .exec()
            .await
            .expect("eager load composite has-many");

        let acme = &accounts[0];
        assert_eq!(acme.name, "Acme");
        assert_eq!(acme.orders().expect_loaded().len(), 2);

        let order = db
            .query(Orders::all())
            .filter(Orders::fields().id().eq(1_i64))
            .first()
            .await
            .expect("load order")
            .expect("order exists");

        let account = db
            .query(order.accounts())
            .first()
            .await
            .expect("load composite belongs-to")
            .expect("account exists");
        assert_eq!(account.name, "Acme");
        assert_eq!(account.tenant, "acme");

        let shipments = db
            .query(Shipments::all())
            .include(Shipments::fields().accounts())
            .order_by(Shipments::fields().id().asc())
            .exec()
            .await
            .expect("eager load optional composite belongs-to");
        assert_eq!(
            shipments[0]
                .accounts()
                .loaded()
                .expect("shipment account was included")
                .name,
            "Acme"
        );
        assert!(shipments[1].accounts().loaded().is_none());
    }

    #[tokio::test]
    async fn supports_nested_includes() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let users = db
            .query(Users::all())
            .include(Users::fields().user_todos().include(Todos::fields().user()))
            .exec()
            .await
            .expect("eager load nested include");

        let todos = users[0].user_todos().loaded().expect("todos were included");
        assert_eq!(todos.len(), 2);
        assert_eq!(
            todos[0]
                .user()
                .loaded()
                .expect("nested user was included")
                .name,
            "Alice"
        );
    }

    #[tokio::test]
    async fn supports_ctes_window_functions_and_derived_joins() {
        use wormx::{Column, Cte, Join, Window, row_number};

        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let active_ids = db
            .query(Users::all())
            .filter(Users::fields().active().eq(true))
            .filter(Users::fields().id().eq(1_i64))
            .select(Users::fields().id());

        let joined = db
            .query(Users::all())
            .with(Cte::new("active_ids", active_ids))
            .join(Join::inner_named(
                "active_ids",
                Users::fields()
                    .id()
                    .eq(Column::<i64>::new(None, "active_ids", "id")),
            ))
            .select(Users::fields().name())
            .exec()
            .await
            .expect("run CTE with join");
        assert_eq!(joined, vec!["Alice".to_owned()]);

        let numbered: Vec<(String, i64)> = db
            .query(Users::all())
            .select((
                Users::fields().name(),
                row_number::<wormx::SqliteDialect>()
                    .over(Window::new().order_by(Users::fields().id().asc())),
            ))
            .exec()
            .await
            .expect("run window function");
        assert_eq!(numbered[0].0, "Alice");
        assert_eq!(numbered[0].1, 1);

        let todo_users = db.query(Todos::all()).select(Todos::fields().user_id());
        let derived = db
            .query(Users::all())
            .filter(Users::fields().id().eq(1_i64))
            .join(Join::inner_derived(
                todo_users,
                "todo_users",
                Users::fields()
                    .id()
                    .eq(Column::<i64>::new(None, "todo_users", "user_id")),
            ))
            .distinct()
            .exec()
            .await
            .expect("run derived-table join");
        assert_eq!(derived.len(), 1);
    }

    #[tokio::test]
    async fn many_to_many_is_a_junction_query() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let team_ids = db
            .query(UserTeams::all())
            .filter(UserTeams::fields().user_id().eq(1_i64))
            .select(UserTeams::fields().team_id());

        let teams = db
            .query(Teams::all())
            .filter(Teams::fields().id().in_query(team_ids))
            .order_by(Teams::fields().id().asc())
            .exec()
            .await
            .expect("load many-to-many through junction table");

        assert_eq!(teams.len(), 2);
        assert_eq!(teams[0].name, "Core");
        assert_eq!(teams[1].name, "Docs");
    }
}
