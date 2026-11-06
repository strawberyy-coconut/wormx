//! Shared row building and validation for batch DML.

use sqlx_core::error::{Error, Result};

use crate::plan::backend::Dialect;
use crate::plan::expr::{Expr, IntoExpr};
use crate::plan::query::Column;

/// A row of column/value pairs for a batch insert or upsert.
///
/// Rows are built by hand or through [`crate::InsertMany::row`] and
/// [`crate::UpsertMany::row`]. Every row in a batch must contain the same
/// columns; the order used by the first row defines the rendered order.
pub struct RowBuilder<D: Dialect> {
    values: Vec<(&'static str, Expr<(), D>)>,
}

impl<D: Dialect> RowBuilder<D> {
    /// Creates an empty row.
    pub fn new() -> Self {
        Self { values: Vec::new() }
    }

    /// Sets one column to a value or expression.
    pub fn value<C, V>(mut self, column: Column<C>, value: V) -> Self
    where
        C: 'static,
        V: IntoExpr<C, D>,
    {
        self.values.push((column.name(), value.into_expr().erase()));
        self
    }

    /// Sets a column to an arbitrary expression.
    pub fn value_expr<C>(mut self, column: Column<C>, value: Expr<(), D>) -> Self {
        self.values.push((column.name(), value));
        self
    }

    pub(crate) fn into_values(self) -> Vec<(&'static str, Expr<(), D>)> {
        self.values
    }
}

impl<D: Dialect> Default for RowBuilder<D> {
    fn default() -> Self {
        Self::new()
    }
}

/// The column order and aligned values produced by [`normalize_rows`].
pub(crate) type NormalizedRows<D> = (Vec<&'static str>, Vec<Vec<Expr<(), D>>>);

/// Validates a batch and aligns every row to the first row's column order.
pub(crate) fn normalize_rows<D: Dialect>(
    rows: Vec<Vec<(&'static str, Expr<(), D>)>>,
) -> Result<NormalizedRows<D>> {
    if rows.is_empty() {
        return Err(Error::InvalidArgument(
            "batch operation requires at least one row".to_owned(),
        ));
    }

    let mut rows = rows.into_iter();
    let first = rows.next().expect("rows is not empty");
    let columns = first.iter().map(|(name, _)| *name).collect::<Vec<_>>();

    if columns.is_empty() {
        if rows.next().is_some() {
            return Err(Error::InvalidArgument(
                "a batch with more than one row requires at least one column".to_owned(),
            ));
        }
        return Ok((columns, vec![Vec::new()]));
    }

    for (index, column) in columns.iter().enumerate() {
        if columns[..index].contains(column) {
            return Err(Error::InvalidArgument(format!(
                "batch row 1 defines column {column:?} more than once"
            )));
        }
    }

    let mut normalized = Vec::with_capacity(rows.len() + 1);
    normalized.push(
        first
            .into_iter()
            .map(|(_, value)| value)
            .collect::<Vec<_>>(),
    );

    for (index, row) in rows.enumerate() {
        let row_number = index + 2;
        let mut slots = (0..columns.len()).map(|_| None).collect::<Vec<_>>();

        for (name, value) in row {
            let position = columns
                .iter()
                .position(|column| *column == name)
                .ok_or_else(|| {
                    Error::InvalidArgument(format!(
                        "batch row {row_number} has unexpected column {name:?}"
                    ))
                })?;
            if slots[position].is_some() {
                return Err(Error::InvalidArgument(format!(
                    "batch row {row_number} defines column {name:?} more than once"
                )));
            }
            slots[position] = Some(value);
        }

        let mut values = Vec::with_capacity(columns.len());
        for (position, slot) in slots.into_iter().enumerate() {
            match slot {
                Some(value) => values.push(value),
                None => {
                    return Err(Error::InvalidArgument(format!(
                        "batch row {row_number} is missing column {:?}",
                        columns[position]
                    )));
                }
            }
        }
        normalized.push(values);
    }

    Ok((columns, normalized))
}
