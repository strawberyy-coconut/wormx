//! Lazy belongs-to and has-many relation handles.

use std::marker::PhantomData;

use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::Result;
use sqlx_core::from_row::FromRow;
use sqlx_core::types::Type;

use super::deferred::Deferred;
use super::query::{Column, IntoSelect, Query, Select};
use crate::exec::Db;
use crate::schema::Table;
use crate::sql::{Dialect, Expr};

/// A single related row referenced by a foreign key.
///
/// `BelongsTo` used by non-null foreign keys is created with [`BelongsTo::new`]. Nullable foreign
/// keys use [`BelongsTo::optional`].
///
/// The generated accessors use [`BelongsTo::with_column`] and
/// [`BelongsTo::optional_with_column`] so the lookup targets the column named by the foreign key,
/// which is not necessarily the target's primary key.
pub struct BelongsTo<T: Table, K> {
    key: Option<K>,
    target_column: Option<&'static str>,
    _marker: PhantomData<T>,
}

impl<T: Table + 'static, K> BelongsTo<T, K> {
    /// Creates a relation from a non-null foreign key, targeting the table's primary key.
    pub const fn new(key: K) -> Self {
        Self::with_column_optional(key, None)
    }

    /// Creates a relation from a non-null foreign key that targets `column` on the related table.
    pub const fn with_column(key: K, column: &'static str) -> Self {
        Self::with_column_optional(key, Some(column))
    }

    /// Creates a relation from a nullable foreign key, targeting the table's primary key.
    pub const fn optional(key: Option<K>) -> Self {
        Self::optional_with_column_optional(key, None)
    }

    /// Creates a relation from a nullable foreign key that targets `column` on the related table.
    pub const fn optional_with_column(key: Option<K>, column: &'static str) -> Self {
        Self::optional_with_column_optional(key, Some(column))
    }

    const fn with_column_optional(key: K, column: Option<&'static str>) -> Self {
        Self {
            key: Some(key),
            target_column: column,
            _marker: PhantomData,
        }
    }

    const fn optional_with_column_optional(key: Option<K>, column: Option<&'static str>) -> Self {
        Self {
            key,
            target_column: column,
            _marker: PhantomData,
        }
    }

    /// Returns the relation key.
    pub fn key(&self) -> Option<&K> {
        self.key.as_ref()
    }

    /// Loads the related row.
    pub async fn first<D>(self, db: &Db<D>) -> Result<Option<T>>
    where
        D: Dialect + Clone,
        K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection:
            sqlx_core::executor::Executor<'c, Database = D::DB>,
    {
        db.query(self).first().await
    }
}

impl<D, T, K> IntoSelect<D> for BelongsTo<T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        let select = db.query(Query::<T>::new());

        let Some(key) = self.key else {
            return select.filter(Expr::<bool, D>::raw("1 = 0"));
        };

        let Some(target_column) = self
            .target_column
            .or_else(|| T::DEF.primary_key.first().copied())
        else {
            return select.filter(Expr::<bool, D>::raw("1 = 0"));
        };

        select.filter(Column::<T>::new(T::DEF.schema, T::DEF.name, target_column).eq::<D, _>(key))
    }
}

/// The many related rows whose foreign key points at a source row.
pub struct HasMany<T: Table, K> {
    key: K,
    foreign_key: &'static str,
    _marker: PhantomData<T>,
}

impl<T: Table + 'static, K> HasMany<T, K> {
    /// Creates a has-many relation.
    pub const fn new(key: K, foreign_key: &'static str) -> Self {
        Self {
            key,
            foreign_key,
            _marker: PhantomData,
        }
    }

    /// Loads the related rows.
    pub async fn exec<D>(self, db: &Db<D>) -> Result<Vec<T>>
    where
        D: Dialect + Clone,
        K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection:
            sqlx_core::executor::Executor<'c, Database = D::DB>,
    {
        db.query(self).exec().await
    }
}

impl<D, T, K> IntoSelect<D> for HasMany<T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        db.query(Query::<T>::new()).filter(
            Column::<T>::new(T::DEF.schema, T::DEF.name, self.foreign_key).eq::<D, _>(self.key),
        )
    }
}

/// A borrowed view of a `HasMany` relation.
///
/// The same value can be used as a query (`db.query(user.todos())` or
/// `user.todos().exec(&db)`) and to inspect the result of a previous
/// `.include(...)` (`user.todos().loaded()`).
pub struct HasManyRef<'a, T: Table, K> {
    key: K,
    foreign_key: &'static str,
    deferred: &'a Deferred<Vec<T>>,
}

impl<'a, T: Table + 'static, K> HasManyRef<'a, T, K> {
    /// Creates a borrowed has-many relation view.
    pub const fn new(key: K, foreign_key: &'static str, deferred: &'a Deferred<Vec<T>>) -> Self {
        Self {
            key,
            foreign_key,
            deferred,
        }
    }

    /// Returns the loaded rows.
    pub fn loaded(self) -> Option<&'a Vec<T>> {
        self.deferred.get()
    }

    /// Returns the loaded rows, panicking when unloaded.
    pub fn expect_loaded(self) -> &'a Vec<T> {
        self.deferred.expect_loaded()
    }

    /// Returns whether the relation has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.deferred.is_loaded()
    }

    /// Returns the deferred value.
    pub fn deferred(self) -> &'a Deferred<Vec<T>> {
        self.deferred
    }
}

impl<'a, T, K> HasManyRef<'a, T, K>
where
    T: Table + 'static,
{
    /// Loads the related rows.
    pub async fn exec<D>(&self, db: &Db<D>) -> Result<Vec<T>>
    where
        D: Dialect + Clone,
        K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection:
            sqlx_core::executor::Executor<'c, Database = D::DB>,
    {
        db.query(HasMany::<T, K>::new(self.key.clone(), self.foreign_key))
            .exec()
            .await
    }
}

impl<D, T, K> IntoSelect<D> for HasManyRef<'_, T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        HasMany::<T, K>::new(self.key, self.foreign_key).into_select(db)
    }
}

/// A borrowed view of a non-null `BelongsTo` relation.
pub struct BelongsToRef<'a, T: Table, K> {
    key: K,
    target_column: Option<&'static str>,
    deferred: &'a Deferred<T>,
}

impl<'a, T: Table + 'static, K> BelongsToRef<'a, T, K> {
    /// Creates a borrowed belongs-to relation view targeting the table's primary key.
    pub const fn new(key: K, deferred: &'a Deferred<T>) -> Self {
        Self {
            key,
            target_column: None,
            deferred,
        }
    }

    /// Creates a borrowed belongs-to relation view targeting `column`.
    pub const fn with_column(key: K, column: &'static str, deferred: &'a Deferred<T>) -> Self {
        Self {
            key,
            target_column: Some(column),
            deferred,
        }
    }

    /// Returns the loaded row.
    pub fn loaded(self) -> Option<&'a T> {
        self.deferred.get()
    }

    /// Returns the loaded row, panicking when unloaded or null.
    pub fn expect_loaded(self) -> &'a T {
        self.deferred.expect_loaded()
    }

    /// Returns whether the relation has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.deferred.is_loaded()
    }

    /// Returns the deferred value.
    pub fn deferred(self) -> &'a Deferred<T> {
        self.deferred
    }
}

impl<'a, T, K> BelongsToRef<'a, T, K>
where
    T: Table + 'static,
{
    /// Loads the related row.
    pub async fn first<D>(&self, db: &Db<D>) -> Result<Option<T>>
    where
        D: Dialect + Clone,
        K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection:
            sqlx_core::executor::Executor<'c, Database = D::DB>,
    {
        let relation = match self.target_column {
            Some(column) => BelongsTo::<T, K>::with_column(self.key.clone(), column),
            None => BelongsTo::<T, K>::new(self.key.clone()),
        };
        db.query(relation).first().await
    }
}

impl<D, T, K> IntoSelect<D> for BelongsToRef<'_, T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        match self.target_column {
            Some(column) => BelongsTo::<T, K>::with_column(self.key, column).into_select(db),
            None => BelongsTo::<T, K>::new(self.key).into_select(db),
        }
    }
}

/// A borrowed view of a nullable `BelongsTo` relation.
pub struct OptionalBelongsToRef<'a, T: Table, K> {
    key: Option<K>,
    target_column: Option<&'static str>,
    deferred: &'a Deferred<Option<T>>,
}

impl<'a, T: Table + 'static, K> OptionalBelongsToRef<'a, T, K> {
    /// Creates a borrowed nullable belongs-to relation view targeting the table's primary key.
    pub const fn new(key: Option<K>, deferred: &'a Deferred<Option<T>>) -> Self {
        Self {
            key,
            target_column: None,
            deferred,
        }
    }

    /// Creates a borrowed nullable belongs-to relation view targeting `column`.
    pub const fn with_column(
        key: Option<K>,
        column: &'static str,
        deferred: &'a Deferred<Option<T>>,
    ) -> Self {
        Self {
            key,
            target_column: Some(column),
            deferred,
        }
    }

    /// Returns the loaded row.
    pub fn loaded(self) -> Option<&'a T> {
        self.deferred.get().and_then(Option::as_ref)
    }

    /// Returns the loaded row, panicking when unloaded or null.
    pub fn expect_loaded(self) -> &'a T {
        self.loaded()
            .expect("relation is not loaded or is null; use `.include(...)` first")
    }

    /// Returns whether the relation has been loaded.
    pub fn is_loaded(&self) -> bool {
        self.deferred.is_loaded()
    }

    /// Returns the deferred value.
    pub fn deferred(self) -> &'a Deferred<Option<T>> {
        self.deferred
    }
}

impl<'a, T, K> OptionalBelongsToRef<'a, T, K>
where
    T: Table + 'static,
{
    /// Loads the related row.
    pub async fn first<D>(&self, db: &Db<D>) -> Result<Option<T>>
    where
        D: Dialect + Clone,
        K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
        T: Send + Unpin + for<'r> FromRow<'r, <D::DB as Database>::Row>,
        <D::DB as Database>::Arguments: sqlx_core::arguments::IntoArguments<D::DB>,
        for<'c> &'c mut <D::DB as Database>::Connection:
            sqlx_core::executor::Executor<'c, Database = D::DB>,
    {
        let relation = match self.target_column {
            Some(column) => BelongsTo::<T, K>::optional_with_column(self.key.clone(), column),
            None => BelongsTo::<T, K>::optional(self.key.clone()),
        };
        db.query(relation).first().await
    }
}

impl<D, T, K> IntoSelect<D> for OptionalBelongsToRef<'_, T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        match self.target_column {
            Some(column) => {
                BelongsTo::<T, K>::optional_with_column(self.key, column).into_select(db)
            }
            None => BelongsTo::<T, K>::optional(self.key).into_select(db),
        }
    }
}
