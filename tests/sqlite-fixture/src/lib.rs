wormx::table!("users");
wormx::table!("todos");
wormx::table!("accounts");
wormx::table!("orders");
wormx::table!("shipments");
wormx::table!("teams");
wormx::table!("user_teams");
wormx::table! {
    "profiles" as Profiles {
        derives(Debug, Clone),
        columns {
            "handle" as profile_handle,
        },
    }
}
wormx::table!("profile_links");
wormx::table!("implicit_parents");
wormx::table!("implicit_children");

wormx::table! {
    "users" as RenamedUser {
        derives(Debug, Clone, serde::Serialize),
        attrs(allow(dead_code), serde(rename_all = "camelCase")),
        relation_attrs(serde(skip)),
        columns {
            "name" as display_name {
                attrs(allow(dead_code), doc = "The user's display name."),
            },
        },
    }
}

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
    use wormx::Table;

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
    fn table_dsl_renames_and_attributes_are_applied() {
        let table = <RenamedUser as wormx::Table>::DEF;
        assert_eq!(table.name, "users");
        assert_eq!(table.primary_key, &["id"]);

        let renamed = RenamedUser::new(
            2,
            "Bob".to_owned(),
            Some("bob@example.com".to_owned()),
            false,
        );
        assert_eq!(renamed.display_name, "Bob");
        assert_eq!(RenamedUser::fields().display_name().name(), "name");

        let json = serde_json::to_value(&renamed).expect("serialize renamed user");
        assert_eq!(json["displayName"], "Bob");
        assert!(json.get("user_todos").is_none(), "{json}");
        assert!(json.get("reviewer_todos").is_none(), "{json}");
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
        let mut writer = SqlWriter::new(dialect);
        let mut expr = wormx::call::<SqliteDialect, Upper>((Expr::column("users", "name"),));

        expr.render(&mut writer).expect("render expression");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "UPPER(\"users\".\"name\")");

        let mut writer = SqlWriter::new(dialect);
        let mut expr = concat(Users::fields().name(), "!");
        expr.render(&mut writer).expect("render operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(\"users\".\"name\") || (?)");

        let mut writer = SqlWriter::new(dialect);
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
    fn renders_case_expressions() {
        use wormx::{Expr, SqlWriter, SqliteDialect};

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(dialect);
        let mut expr = Expr::<i32, SqliteDialect>::case()
            .when(Users::fields().active().eq(true), 1)
            .when(Users::fields().name().eq("Alice"), 2)
            .otherwise(0);

        expr.render(&mut writer).expect("render CASE expression");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "CASE WHEN (\"users\".\"active\") = (?) THEN ? WHEN (\"users\".\"name\") = (?) THEN ? ELSE ? END"
        );

        let mut writer = SqlWriter::new(dialect);
        let mut expr = Expr::<i32, SqliteDialect>::case()
            .when(Users::fields().active().eq(true), 1)
            .end();

        expr.render(&mut writer).expect("render CASE without ELSE");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "CASE WHEN (\"users\".\"active\") = (?) THEN ? END");
    }

    #[test]
    fn renders_cast_expressions() {
        use wormx::{Expr, SqlWriter, SqliteDialect};

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(dialect);
        let mut expr = Users::fields()
            .name()
            .cast::<SqliteDialect>("NVARCHAR(200)");

        expr.render(&mut writer).expect("render column CAST");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "CAST(\"users\".\"name\" AS NVARCHAR(200))");

        let mut writer = SqlWriter::new(dialect);
        let mut expr = Expr::<String, SqliteDialect>::raw("name").cast("TEXT");

        expr.render(&mut writer).expect("render expression CAST");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "CAST(name AS TEXT)");
    }

    #[test]
    fn renders_large_projection_and_function_tuples() {
        use wormx::{Expr, Function, SqlWriter, SqliteDialect, write_projection};

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(dialect);
        let mut projection = (
            Users::fields().id(),
            Users::fields().name(),
            Users::fields().email(),
            Users::fields().active(),
            Users::fields().id(),
            Users::fields().name(),
            Users::fields().email(),
            Users::fields().active(),
            Users::fields().id(),
            Users::fields().name(),
            Users::fields().email(),
            Users::fields().active(),
            Users::fields().id(),
            Users::fields().name(),
            Users::fields().email(),
            Users::fields().active(),
        );

        write_projection(&mut writer, &mut projection).expect("render projection tuple");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "\"users\".\"id\", \"users\".\"name\", \"users\".\"email\", \"users\".\"active\", \
             \"users\".\"id\", \"users\".\"name\", \"users\".\"email\", \"users\".\"active\", \
             \"users\".\"id\", \"users\".\"name\", \"users\".\"email\", \"users\".\"active\", \
             \"users\".\"id\", \"users\".\"name\", \"users\".\"email\", \"users\".\"active\""
        );

        struct SixteenArgs;

        impl Function<SqliteDialect> for SixteenArgs {
            type Args = (
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
                Expr<String, SqliteDialect>,
            );
            type Output = String;
            const NAME: &'static str = "SIXTEEN";
        }

        let mut writer = SqlWriter::new(dialect);
        let mut expr = wormx::call::<SqliteDialect, SixteenArgs>((
            Expr::raw("a"),
            Expr::raw("b"),
            Expr::raw("c"),
            Expr::raw("d"),
            Expr::raw("e"),
            Expr::raw("f"),
            Expr::raw("g"),
            Expr::raw("h"),
            Expr::raw("i"),
            Expr::raw("j"),
            Expr::raw("k"),
            Expr::raw("l"),
            Expr::raw("m"),
            Expr::raw("n"),
            Expr::raw("o"),
            Expr::raw("p"),
        ));

        expr.render(&mut writer)
            .expect("render function args tuple");
        let (sql, _args) = writer.finish();
        assert_eq!(
            sql,
            "SIXTEEN(a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p)"
        );
    }

    #[test]
    fn custom_prefix_operator_groups_its_operand() {
        use wormx::{Expr, SqlWriter, SqliteDialect};

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(dialect);
        let mut expr = not_(Expr::<bool, SqliteDialect>::raw("a").and(Expr::raw("b")));

        expr.render(&mut writer).expect("render prefix operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "NOT ((a) AND (b))");

        let mut writer = SqlWriter::new(dialect);
        let mut expr = explicit_not(Expr::<bool, SqliteDialect>::raw("a"));
        expr.render(&mut writer).expect("render operator! prefix");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "NOT (a)");

        let mut writer = SqlWriter::new(dialect);
        let mut expr = raw_concat(Expr::<String, SqliteDialect>::raw("a"), Expr::raw("b"));
        expr.render(&mut writer).expect("render ungrouped operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "a || b");

        let mut writer = SqlWriter::new(dialect);
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

        let mut writer = SqlWriter::new(dialect);
        expr.render(&mut writer).expect("render nested operators");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(a) + ((b) * (c))");

        let mut writer = SqlWriter::new(dialect);
        let mut grouped = Expr::<i64, SqliteDialect>::raw("a + b").group();
        grouped.render(&mut writer).expect("render grouped expr");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(a + b)");

        let mut writer = SqlWriter::new(dialect);
        let mut postfix = Expr::<i64, SqliteDialect>::postfix(Expr::raw("x"), "::int");
        postfix
            .render(&mut writer)
            .expect("render postfix operator");
        let (sql, _args) = writer.finish();
        assert_eq!(sql, "(x) ::int");

        let mut writer = SqlWriter::new(dialect);
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
                writer: &mut SqlWriter<SqliteDialect>,
            ) -> sqlx::Result<()>
            where
                P: wormx::Projection<SqliteDialect>,
            {
                writer.write_select(plan.distinct);
                wormx::write_projection(writer, plan.projection)?;
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
                writer: &mut SqlWriter<SqliteDialect>,
                distinct: bool,
            ) -> sqlx::Result<()>
            where
                P: wormx::Projection<SqliteDialect> + wormx::CountProjection<SqliteDialect>,
            {
                SqlSelect.write_count(plan, writer, distinct)
            }

            fn write_count_star(
                &self,
                plan: wormx::CountPlan<'_, SqliteDialect>,
                writer: &mut SqlWriter<SqliteDialect>,
            ) -> sqlx::Result<()> {
                SqlSelect.write_count_star(plan, writer)
            }
        }

        let dialect = SqliteDialect;
        let mut writer = SqlWriter::new(dialect);
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
                    ctes: &mut [],
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
            .query(Users::all().filter(Users::fields().active().eq(true)))
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
                writer: &mut wormx::SqlWriter<TrackingDialect>,
            ) -> sqlx::Result<()>
            where
                P: wormx::Projection<TrackingDialect>,
            {
                SELECT_CALLS.fetch_add(1, Ordering::SeqCst);
                SqlSelect.write(plan, writer)
            }

            fn write_count<P>(
                &self,
                plan: wormx::SelectPlan<'_, TrackingDialect, P>,
                writer: &mut wormx::SqlWriter<TrackingDialect>,
                distinct: bool,
            ) -> sqlx::Result<()>
            where
                P: wormx::Projection<TrackingDialect> + wormx::CountProjection<TrackingDialect>,
            {
                SELECT_CALLS.fetch_add(1, Ordering::SeqCst);
                SqlSelect.write_count(plan, writer, distinct)
            }

            fn write_count_star(
                &self,
                plan: wormx::CountPlan<'_, TrackingDialect>,
                writer: &mut wormx::SqlWriter<TrackingDialect>,
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

    #[tokio::test]
    async fn loads_belongs_to_on_unique_non_primary_key() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let link = db
            .query(ProfileLinks::get(1_i64))
            .first()
            .await
            .expect("load profile link")
            .expect("profile link exists");

        let profile = db
            .query(link.profiles())
            .first()
            .await
            .expect("load belongs-to by unique column")
            .expect("profile exists");
        assert_eq!(profile.name, "Alice Profile");

        let profile = db
            .query(link.profiles())
            .first()
            .await
            .expect("query belongs-to by unique column")
            .expect("profile exists");
        assert_eq!(profile.profile_handle, "alice");

        let links = db
            .query(ProfileLinks::all())
            .include(ProfileLinks::fields().profiles())
            .order_by(ProfileLinks::fields().id().asc())
            .exec()
            .await
            .expect("eager load belongs-to by unique column");

        assert_eq!(
            links[0]
                .profiles()
                .loaded()
                .expect("profile was included")
                .name,
            "Alice Profile"
        );
    }

    #[tokio::test]
    async fn loads_sqlite_implicit_foreign_key_references() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        assert!(
            ImplicitChildren::DEF
                .relations
                .iter()
                .any(|relation| relation.name == "parent")
        );
        assert!(
            ImplicitParents::DEF
                .relations
                .iter()
                .any(|relation| relation.name == "implicit_children")
        );

        let children = db
            .query(ImplicitChildren::all())
            .include(ImplicitChildren::fields().parent())
            .order_by(ImplicitChildren::fields().id().asc())
            .exec()
            .await
            .expect("eager load implicit belongs-to");

        assert_eq!(children.len(), 2);
        assert_eq!(
            children[0]
                .parent()
                .loaded()
                .expect("implicit parent was included")
                .name,
            "Parent"
        );

        let parents = db
            .query(ImplicitParents::all())
            .include(ImplicitParents::fields().implicit_children())
            .exec()
            .await
            .expect("eager load implicit has-many");

        assert_eq!(
            parents[0]
                .implicit_children()
                .loaded()
                .expect("implicit children were included")
                .len(),
            2
        );
    }

    #[tokio::test]
    async fn include_assigns_children_to_duplicate_parent_rows() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let users = db
            .query(Users::all())
            .join(wormx::Join::inner(
                UserTeams::fields().table(),
                Users::fields().id().eq(UserTeams::fields().user_id()),
            ))
            .include(Users::fields().user_todos())
            .exec()
            .await
            .expect("eager load has-many with duplicate parents");

        assert_eq!(users.len(), 2);
        for user in &users {
            assert_eq!(
                user.user_todos()
                    .loaded()
                    .expect("todos were included")
                    .len(),
                2
            );
        }
    }

    #[tokio::test]
    async fn distinct_and_grouped_counts_count_result_rows() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let distinct_users = db
            .query(Todos::all())
            .distinct()
            .select(Todos::fields().user_id())
            .count()
            .await
            .expect("count distinct projection");
        assert_eq!(distinct_users, 1);

        let groups = db
            .query(Todos::all())
            .select(Todos::fields().user_id())
            .group_by(Todos::fields().user_id())
            .count()
            .await
            .expect("count grouped projection");
        assert_eq!(groups, 1);
    }

    #[tokio::test]
    async fn offset_without_limit_works_on_sqlite() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let todos = db
            .query(Todos::all())
            .order_by(Todos::fields().id().asc())
            .offset(1)
            .exec()
            .await
            .expect("run offset without limit");

        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].id, 2);
    }

    #[tokio::test]
    async fn supports_full_join_on_sqlite() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let rows: Vec<i64> = db
            .query(Users::all())
            .join(wormx::Join::full(
                Todos::alias("t").table(),
                Users::fields().id().eq(Todos::alias("t").user_id()),
            ))
            .select(Users::fields().id())
            .exec()
            .await
            .expect("run FULL JOIN");

        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn invalid_update_and_upsert_are_rejected() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let error = db
            .update::<Users>()
            .filter(Users::fields().id().eq(1_i64))
            .exec()
            .await
            .expect_err("UPDATE without assignments must fail");
        assert!(error.to_string().contains("UPDATE requires"));

        let error = db
            .upsert::<Users>()
            .value(Users::fields().id(), 1_i64)
            .conflict_on(Users::fields().id())
            .exec()
            .await
            .expect_err("UPSERT without assignments must fail");
        assert!(error.to_string().contains("UPSERT requires"));

        let error = db
            .upsert::<Users>()
            .value(Users::fields().id(), 1_i64)
            .update(Users::fields().name(), "Alice")
            .exec()
            .await
            .expect_err("UPSERT without conflict columns must fail");
        assert!(error.to_string().contains("conflict column"));

        let error = db
            .query(Users::all())
            .include(Users::fields().user_todos())
            .select(Users::fields().id())
            .exec()
            .await
            .expect_err("INCLUDE followed by SELECT must fail");
        assert!(error.to_string().contains("after .select"));
    }

    #[tokio::test]
    async fn a_custom_dialect_reuses_the_plan_with_its_own_renderer() {
        use wormx::{
            ColumnRef, CountProjection, Dialect, ExprKind, Projection, ProjectionVisitor,
            SelectPlan, SelectRenderer, SqlSyntax, Writer,
        };

        #[derive(Debug, Clone, Copy, Default)]
        struct Cypherish;

        #[derive(Default)]
        struct CypherSelect;

        impl Dialect for Cypherish {
            type DB = sqlx::Sqlite;
            type Select = CypherSelect;
            type Insert = wormx::SqlInsert;
            type Update = wormx::SqlUpdate;
            type Delete = wormx::SqlDelete;
            type Upsert = wormx::SqlUpsert;

            fn quote_ident(&self, name: &str) -> String {
                format!("`{name}`")
            }
        }

        impl SqlSyntax for Cypherish {}

        struct CypherProjection<'a> {
            writer: &'a mut Writer<Cypherish>,
        }

        impl ProjectionVisitor<Cypherish> for CypherProjection<'_> {
            fn star(&mut self) -> sqlx::Result<()> {
                self.writer.push("*");
                Ok(())
            }

            fn column(&mut self, column: &ColumnRef) -> sqlx::Result<()> {
                self.writer.push(&column.column);
                Ok(())
            }

            fn expr(&mut self, _expr: &mut ExprKind<Cypherish>) -> sqlx::Result<()> {
                self.writer.push("<expr>");
                Ok(())
            }

            fn separator(&mut self) -> sqlx::Result<()> {
                self.writer.push(", ");
                Ok(())
            }

            fn begin_count(&mut self, _distinct: bool) -> sqlx::Result<()> {
                self.writer.push("count(");
                Ok(())
            }

            fn end_count(&mut self) -> sqlx::Result<()> {
                self.writer.push(")");
                Ok(())
            }
        }

        impl SelectRenderer<Cypherish> for CypherSelect {
            fn write<P>(
                &self,
                plan: SelectPlan<'_, Cypherish, P>,
                writer: &mut Writer<Cypherish>,
            ) -> sqlx::Result<()>
            where
                P: Projection<Cypherish>,
            {
                writer.push("MATCH (n) WHERE ");
                writer.push(&format!("{} filter(s) RETURN ", plan.filters.len()));
                plan.projection.visit(&mut CypherProjection { writer })
            }

            fn write_count<P>(
                &self,
                plan: SelectPlan<'_, Cypherish, P>,
                writer: &mut Writer<Cypherish>,
                distinct: bool,
            ) -> sqlx::Result<()>
            where
                P: Projection<Cypherish> + CountProjection<Cypherish>,
            {
                writer.push("RETURN count(");
                if distinct {
                    writer.push("distinct ");
                }
                plan.projection.visit(&mut CypherProjection { writer })?;
                writer.push(")");
                Ok(())
            }

            fn write_count_star(
                &self,
                _plan: wormx::CountPlan<'_, Cypherish>,
                writer: &mut Writer<Cypherish>,
            ) -> sqlx::Result<()> {
                writer.push("RETURN count(*)");
                Ok(())
            }
        }

        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, Cypherish);

        let mut query = db
            .query(Users::all())
            .filter(Users::fields().id().gt(0_i64))
            .select(Users::fields().id());

        let (command, _args) = query.render().expect("render with custom dialect");
        assert_eq!(command, "MATCH (n) WHERE 1 filter(s) RETURN id");
    }

    #[tokio::test]
    async fn batch_insert_upsert_update_and_delete() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let ids = [80_i64, 81, 82];
        db.delete::<Users>()
            .filter(Users::fields().id().in_list(ids))
            .exec()
            .await
            .expect("clean prior rows");

        let inserted = db
            .insert_many::<Users>()
            .row(|row| {
                row.value(Users::fields().id(), 80_i64)
                    .value(Users::fields().name(), "Batch Alice")
                    .value(Users::fields().email(), None::<String>)
                    .value(Users::fields().active(), true)
            })
            .row(|row| {
                row.value(Users::fields().id(), 81_i64)
                    .value(Users::fields().name(), "Batch Bob")
                    .value(Users::fields().email(), None::<String>)
                    .value(Users::fields().active(), false)
            })
            .exec_returning()
            .await
            .expect("batch insert");
        assert_eq!(inserted.len(), 2);
        assert_eq!(inserted.iter().filter(|user| user.active).count(), 1);

        let upserted = db
            .upsert_many::<Users>()
            .row(|row| {
                row.value(Users::fields().id(), 80_i64)
                    .value(Users::fields().name(), "Batch Alice 2")
                    .value(Users::fields().email(), None::<String>)
                    .value(Users::fields().active(), true)
            })
            .row(|row| {
                row.value(Users::fields().id(), 82_i64)
                    .value(Users::fields().name(), "Batch Carol")
                    .value(Users::fields().email(), None::<String>)
                    .value(Users::fields().active(), true)
            })
            .conflict_on(Users::fields().id())
            .update_from_input(Users::fields().name())
            .update_from_input(Users::fields().active())
            .exec_returning()
            .await
            .expect("batch upsert");
        assert_eq!(upserted.len(), 2);

        let alice = db
            .query(Users::get(80_i64))
            .first()
            .await
            .expect("load upserted user")
            .expect("user exists");
        assert_eq!(alice.name, "Batch Alice 2");

        let carol = db
            .query(Users::get(82_i64))
            .first()
            .await
            .expect("load inserted user")
            .expect("user exists");
        assert_eq!(carol.name, "Batch Carol");

        let updated = db
            .update_many::<Users>()
            .key(Users::fields().id())
            .row(|row| {
                row.key(80_i64)
                    .value(Users::fields().name(), "Batch Alice 3")
                    .value(Users::fields().active(), true)
            })
            .row(|row| row.key(81_i64).value(Users::fields().name(), "Batch Bob 2"))
            .exec_returning()
            .await
            .expect("batch update");
        assert_eq!(updated.len(), 2);

        let alice = db
            .query(Users::get(80_i64))
            .first()
            .await
            .expect("load updated user")
            .expect("user exists");
        assert_eq!(alice.name, "Batch Alice 3");

        let bob = db
            .query(Users::get(81_i64))
            .first()
            .await
            .expect("load updated user")
            .expect("user exists");
        assert_eq!(bob.name, "Batch Bob 2");
        assert!(!bob.active, "an omitted column keeps its existing value");

        db.delete::<Users>()
            .filter(Users::fields().id().in_list(ids))
            .exec()
            .await
            .expect("batch delete");

        for id in ids {
            assert!(
                db.query(Users::get(id))
                    .first()
                    .await
                    .expect("load deleted user")
                    .is_none()
            );
        }
    }

    #[tokio::test]
    async fn batch_writes_report_validation_errors() {
        let pool = sqlx::SqlitePool::connect(env!("DATABASE_URL"))
            .await
            .expect("connect to fixture database");
        let db = wormx::Db::new(pool, wormx::SqliteDialect);

        let error = db
            .insert_many::<Users>()
            .exec()
            .await
            .expect_err("empty batch is rejected");
        assert!(error.to_string().contains("at least one row"));

        let error = db
            .insert_many::<Users>()
            .row(|row| {
                row.value(Users::fields().id(), 90_i64)
                    .value(Users::fields().name(), "Batch Alice")
            })
            .row(|row| row.value(Users::fields().id(), 91_i64))
            .exec()
            .await
            .expect_err("mismatched columns are rejected");
        assert!(error.to_string().contains("missing column"));

        let error = db
            .update_many::<Users>()
            .key(Users::fields().id())
            .row(|row| row.key(90_i64).value(Users::fields().name(), "A"))
            .row(|row| row.key(90_i64).value(Users::fields().name(), "B"))
            .exec()
            .await
            .expect_err("duplicate keys are rejected");
        assert!(error.to_string().contains("repeats an earlier key"));

        let error = db
            .update_many::<Users>()
            .key(Users::fields().id())
            .row(|row| row.value(Users::fields().name(), "A"))
            .exec()
            .await
            .expect_err("missing keys are rejected");
        assert!(error.to_string().contains("missing a key"));
    }
}
