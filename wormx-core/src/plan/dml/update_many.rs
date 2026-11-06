//! The per-row `UPDATE` builder.
//!
//! A batch update is rendered as one `UPDATE` statement whose assignments use
//! `CASE` expressions keyed by a single column.

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::{Error, Result};
use sqlx_core::executor::Executor;
use sqlx_core::from_row::FromRow;
use sqlx_core::query::query_with;
use sqlx_core::query_as::query_as_with;
use sqlx_core::sql_str::{AssertSqlSafe, SqlSafeStr};
use sqlx_core::types::Type;

use crate::exec::{Db, Transaction};
use crate::plan::backend::{Dialect, SupportsReturning, SupportsSourceSuffix};
use crate::plan::expr::{ColumnRef, Expr, ExprKind, IntoExpr};
use crate::plan::join::TableSource;
use crate::plan::query::{Column, UpdatePlan, UpdateRenderer};
use crate::schema::Table;

/// One row in a per-row batch update.
pub struct UpdateRow<D: Dialect, K> {
    key: Option<K>,
    values: Vec<(&'static str, ColumnRef, Expr<(), D>)>,
}

impl<D: Dialect, K> UpdateRow<D, K> {
    /// Creates an empty row.
    pub fn new() -> Self {
        Self {
            key: None,
            values: Vec::new(),
        }
    }

    /// Sets the key value used by the `CASE` branches.
    pub fn key(mut self, key: K) -> Self {
        self.key = Some(key);
        self
    }

    /// Sets one column to a value or expression.
    pub fn value<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        let name = column.name();
        let mut column = column.column_ref();
        column.alias = None;
        self.values.push((name, column, value.into_expr().erase()));
        self
    }

    /// Sets a column to an arbitrary expression.
    pub fn value_expr<C>(mut self, column: Column<C>, value: Expr<(), D>) -> Self {
        let name = column.name();
        let mut column = column.column_ref();
        column.alias = None;
        self.values.push((name, column, value));
        self
    }
}

impl<D: Dialect, K> Default for UpdateRow<D, K> {
    fn default() -> Self {
        Self::new()
    }
}

/// A pending per-row `UPDATE` before its key column has been chosen.
pub struct UpdateManyStart<D: Dialect + Clone, T: Table> {
    db: Db<D>,
    source: TableSource,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table> UpdateManyStart<D, T> {
    pub(crate) fn new(db: Db<D>) -> Self {
        Self {
            db,
            source: TableSource::new(T::DEF, None),
            _marker: PhantomData,
        }
    }

    /// Attaches an opaque, dialect-defined suffix to the target table.
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self
    where
        D: SupportsSourceSuffix,
    {
        self.source.suffix = Some(suffix);
        self
    }

    /// Chooses the column used to match each row's key.
    pub fn key<K>(self, key: Column<K>) -> UpdateMany<D, T, K> {
        UpdateMany {
            db: self.db,
            source: self.source,
            key,
            rows: Vec::new(),
            _marker: PhantomData,
        }
    }
}

/// A pending per-row `UPDATE`, rendered as one statement.
pub struct UpdateMany<D: Dialect + Clone, T: Table, K> {
    db: Db<D>,
    source: TableSource,
    key: Column<K>,
    rows: Vec<UpdateRow<D, K>>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table, K> UpdateMany<D, T, K> {
    /// Adds one row.
    pub fn row<F>(mut self, build: F) -> Self
    where
        F: FnOnce(UpdateRow<D, K>) -> UpdateRow<D, K>,
    {
        self.rows.push(build(UpdateRow::new()));
        self
    }

    /// Adds one row per item produced by `items`.
    pub fn rows<I, F>(mut self, items: I, mut build: F) -> Self
    where
        I: IntoIterator,
        F: FnMut(I::Item, UpdateRow<D, K>) -> UpdateRow<D, K>,
    {
        for item in items {
            self = self.row(|row| build(item, row));
        }
        self
    }

    /// Attaches an opaque, dialect-defined suffix to the target table.
    pub fn with_source_suffix(mut self, suffix: &'static str) -> Self
    where
        D: SupportsSourceSuffix,
    {
        self.source.suffix = Some(suffix);
        self
    }

    /// Executes the statement.
    pub async fn exec(self) -> Result<<D::DB as Database>::QueryResult>
    where
        K: Clone + PartialEq + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let pool = self.db.pool().clone();
        let (sql, arguments) = self.render(false)?;
        query_with::<D::DB, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .execute(&pool)
            .await
    }

    /// Executes the statement inside a transaction.
    pub async fn exec_in(
        self,
        tx: &mut Transaction<'_, D>,
    ) -> Result<<D::DB as Database>::QueryResult>
    where
        K: Clone + PartialEq + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(false)?;
        query_with::<D::DB, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .execute(tx.connection())
            .await
    }

    /// Executes the statement and decodes every updated row.
    pub async fn exec_returning(self) -> Result<Vec<T>>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        K: Clone + PartialEq + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let pool = self.db.pool().clone();
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_all(&pool)
            .await
    }

    /// Executes the statement inside a transaction and decodes every updated
    /// row.
    pub async fn exec_returning_in(self, tx: &mut Transaction<'_, D>) -> Result<Vec<T>>
    where
        D: SupportsReturning,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        K: Clone + PartialEq + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection: Executor<'c, Database = D::DB>,
    {
        let (sql, arguments) = self.render(true)?;
        query_as_with::<D::DB, T, _>(AssertSqlSafe(sql).into_sql_str(), arguments)
            .fetch_all(tx.connection())
            .await
    }

    fn render(mut self, returning: bool) -> Result<(String, <D::DB as Database>::Arguments)>
    where
        K: Clone + PartialEq + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
    {
        if self.rows.is_empty() {
            return Err(Error::InvalidArgument(
                "UPDATE batch requires at least one row".to_owned(),
            ));
        }

        let mut keys = Vec::with_capacity(self.rows.len());
        for (index, row) in self.rows.iter_mut().enumerate() {
            match row.key.take() {
                Some(key) => keys.push(key),
                None => {
                    return Err(Error::InvalidArgument(format!(
                        "update batch row {} is missing a key; call .key(...)",
                        index + 1
                    )));
                }
            }
        }

        for index in 0..keys.len() {
            if keys[..index].contains(&keys[index]) {
                return Err(Error::InvalidArgument(format!(
                    "update batch row {} repeats an earlier key",
                    index + 1
                )));
            }
        }

        struct Pending<D: Dialect, K> {
            name: &'static str,
            column: ColumnRef,
            values: Vec<(K, Expr<(), D>)>,
        }

        let mut columns: Vec<Pending<D, K>> = Vec::new();
        for (index, row) in self.rows.iter_mut().enumerate() {
            let key = keys[index].clone();
            let mut seen: Vec<&'static str> = Vec::new();
            for (name, column, value) in row.values.drain(..) {
                if seen.contains(&name) {
                    return Err(Error::InvalidArgument(format!(
                        "update batch row {} defines column {name:?} more than once",
                        index + 1
                    )));
                }
                seen.push(name);

                if let Some(pending) = columns.iter_mut().find(|pending| pending.name == name) {
                    pending.values.push((key.clone(), value));
                } else {
                    columns.push(Pending {
                        name,
                        column,
                        values: vec![(key.clone(), value)],
                    });
                }
            }
        }

        if columns.is_empty() {
            return Err(Error::InvalidArgument(
                "UPDATE requires at least one assignment; call .value(...) or .value_expr(...)"
                    .to_owned(),
            ));
        }

        let key_column =
            Column::<K>::with_alias(self.key.schema(), self.key.table(), None, self.key.name());
        let key = key_column.column_ref();
        let row_count = self.rows.len();
        let mut assignments = Vec::with_capacity(columns.len());
        for pending in columns {
            let full = pending.values.len() == row_count;
            let mut case = Expr::<(), D>::case();
            for (key_value, value) in pending.values {
                let condition =
                    Expr::<K, D>::from_kind(ExprKind::Column(key.clone())).eq(key_value);
                case = case.when(condition, value);
            }
            let value = if full {
                case.end().erase()
            } else {
                case.otherwise(Expr::<(), D>::from_kind(ExprKind::Column(pending.column)))
            };
            assignments.push((pending.name, value));
        }

        let mut filters = vec![key_column.in_list(keys)];

        let mut writer = self.db.writer();
        D::Update::default().write(
            UpdatePlan {
                table: self.source,
                assignments: &mut assignments,
                filters: &mut filters,
                returning,
            },
            &mut writer,
        )?;
        Ok(writer.finish())
    }
}
