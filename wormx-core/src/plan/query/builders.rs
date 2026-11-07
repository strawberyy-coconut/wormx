use std::marker::PhantomData;

use sqlx_core::encode::Encode;
use sqlx_core::types::Type;

use super::column::Column;
use super::select::Select;
use crate::exec::Db;
use crate::plan::backend::Dialect;
use crate::plan::expr::Expr;
use crate::schema::Table;

/// A dialect-neutral table query.
pub struct Query<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> Query<T> {
    /// Creates an empty table query.
    pub const fn new() -> Self {
        Self {
            _marker: PhantomData,
        }
    }

    /// Adds a filter to the table query.
    pub fn filter<D>(self, filter: Expr<bool, D>) -> Filtered<D, T>
    where
        D: Dialect + Clone,
        T: Table + 'static,
    {
        Filtered::new(filter)
    }
}

impl<T> Default for Query<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// A lookup by primary key.
pub struct Get<T, K> {
    key: K,
    _marker: PhantomData<fn() -> T>,
}

impl<T, K> Get<T, K> {
    pub const fn new(key: K) -> Self {
        Self {
            key,
            _marker: PhantomData,
        }
    }
}

/// A table query with a filter, independent of a database handle.
pub struct Filtered<D: Dialect + Clone, T: Table + 'static> {
    filter: Expr<bool, D>,
    _marker: PhantomData<fn() -> T>,
}

impl<D: Dialect + Clone, T: Table + 'static> Filtered<D, T> {
    /// Creates a filtered table query.
    pub(crate) fn new(filter: Expr<bool, D>) -> Self {
        Self {
            filter,
            _marker: PhantomData,
        }
    }
}

impl<D: Dialect + Clone, T: Table + 'static> IntoSelect<D> for Filtered<D, T> {
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        db.query(Query::<T>::new()).filter(self.filter)
    }
}

/// Converts a table query into a dialect-bound `Select`.
pub trait IntoSelect<D: Dialect + Clone> {
    type Table;

    fn into_select(self, db: &Db<D>) -> Select<D, Self::Table>;
}

impl<D, T> IntoSelect<D> for Query<T>
where
    D: Dialect + Clone,
    T: Table + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        Select::new(db.clone())
    }
}

impl<D, T, K> IntoSelect<D> for Get<T, K>
where
    D: Dialect + Clone,
    T: Table + 'static,
    K: Clone + for<'q> Encode<'q, D::DB> + Type<D::DB> + Send + 'static,
{
    type Table = T;

    fn into_select(self, db: &Db<D>) -> Select<D, T> {
        let select = db.query(Query::<T>::new());

        match T::DEF.primary_key.first() {
            Some(primary_key) => select.filter(
                Column::<T>::new(T::DEF.schema, T::DEF.name, primary_key).eq::<D, _>(self.key),
            ),
            None => select.filter(Expr::<bool, D>::literal(false)),
        }
    }
}
