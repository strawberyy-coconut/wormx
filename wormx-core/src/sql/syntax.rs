//! SQL dialect traits and generic spelling hooks.

use sqlx_core::database::Database;

use super::writer::SqlWriter;
use crate::plan::query::{
    DeleteRenderer, InsertRenderer, SelectRenderer, UpdateRenderer, UpsertRenderer,
};

/// Generic SQL spelling hooks used by the built-in renderers.
///
/// `Dialect` stays intentionally small: it identifies the database and chooses
/// renderers. This trait holds the spelling defaults that most SQL dialects
/// share. A driver implements it only when its syntax differs from the default.
///
/// The methods use [`SqlWriter`] and therefore require `Self: Dialect` at the
/// call site. Core's `Dialect` supertrait relationship supplies that bound.
pub trait SqlSyntax: 'static + Sized + Send + Sync {
    /// Appends the dialect's LIMIT/OFFSET syntax.
    fn write_limit_offset(
        &self,
        _has_order: bool,
        limit: Option<u64>,
        offset: Option<u64>,
        out: &mut String,
    ) where
        Self: Dialect,
    {
        if let Some(limit) = limit {
            out.push_str(&format!(" LIMIT {limit}"));
        }
        if let Some(offset) = offset {
            out.push_str(&format!(" OFFSET {offset}"));
        }
    }

    fn write_select(distinct: bool, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("SELECT ");
        if distinct {
            writer.push("DISTINCT ");
        }
    }

    fn write_from(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" FROM ");
    }

    fn write_insert(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("INSERT INTO ");
    }

    fn write_insert_default(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" DEFAULT VALUES");
    }

    fn write_values(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" VALUES (");
    }

    fn write_update(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("UPDATE ");
    }

    fn write_set(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" SET ");
    }

    fn write_equals(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" = ");
    }

    fn write_delete(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("DELETE FROM ");
    }

    fn write_returning(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" RETURNING ");
        writer.write_star();
    }

    fn write_on_conflict(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" ON CONFLICT ");
    }

    fn write_do_update_set(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" DO UPDATE SET ");
    }

    fn write_where(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" WHERE ");
    }

    fn write_and(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" AND ");
    }

    fn write_or(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" OR ");
    }

    fn write_join(kind: crate::plan::join::JoinKind, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(match kind {
            crate::plan::join::JoinKind::Inner => " JOIN ",
            crate::plan::join::JoinKind::Left => " LEFT JOIN ",
            crate::plan::join::JoinKind::Right => " RIGHT JOIN ",
            crate::plan::join::JoinKind::Full => " FULL JOIN ",
        });
    }

    fn write_on(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" ON ");
    }

    fn write_group_by(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" GROUP BY ");
    }

    fn write_partition_by(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("PARTITION BY ");
    }

    fn write_over(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" OVER (");
    }

    fn write_having(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" HAVING ");
    }

    fn write_order_by(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" ORDER BY ");
    }

    fn write_asc(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" ASC");
    }

    fn write_desc(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" DESC");
    }

    fn write_as(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" AS ");
    }

    fn write_comma(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(", ");
    }

    fn write_star(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("*");
    }

    fn write_count_star(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("COUNT(*)");
    }

    /// Writes the name of the `COUNT` function, without its opening parenthesis.
    ///
    /// The renderer writes the opening parenthesis and any `DISTINCT` keyword
    /// itself, so implementations must not include `(` in this hook.
    fn write_count_start(_distinct: bool, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("COUNT");
    }

    /// Writes a function name, without its opening parenthesis.
    ///
    /// The caller writes the argument list parentheses, so implementations must
    /// not include `(` in this hook.
    fn write_function_start(name: &str, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(name);
    }

    /// Writes an opaque, dialect-defined suffix attached to a table source or
    /// DML target.
    ///
    /// Core assigns no meaning to the text. The default ignores it; a dialect
    /// that supports source suffixes interprets and renders it.
    fn write_source_suffix(&self, _suffix: &str, _writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
    }

    /// Writes an opaque, dialect-defined tail for a statement.
    ///
    /// Core assigns no meaning to the text. The default ignores it; a dialect
    /// that supports a trailing clause interprets and renders it.
    fn write_tail(&self, _tail: &str, _writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
    }

    fn write_operator(operator: &str, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.write_space();
        writer.push(operator);
        writer.write_space();
    }

    /// Writes the `NOT` keyword without an opening parenthesis.
    ///
    /// [`crate::sql::expr::NotExpr`] writes the parentheses around the operand,
    /// so implementations must not include `(` in this hook.
    fn write_not(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("NOT ");
    }

    fn write_in(negated: bool, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(if negated { " NOT IN (" } else { " IN (" });
    }

    fn write_is_null(negated: bool, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(if negated { " IS NOT NULL" } else { " IS NULL" });
    }

    fn write_like(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" LIKE ");
    }

    fn write_boolean_literal(value: bool, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(if value { "1 = 1" } else { "1 = 0" });
    }

    fn write_group_start(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push("(");
    }

    fn write_group_end(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(")");
    }

    fn write_identifier_separator(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(".");
    }

    fn write_space(writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        writer.push(" ");
    }
}

/// A backend dialect.
///
/// `Dialect` identifies the database and chooses one renderer per operation.
/// Generic SQL spelling lives in [`SqlSyntax`]; dialect-specific syntax that is
/// not covered there belongs in that driver's renderers or extension traits.
pub trait Dialect: SqlSyntax + Clone {
    type DB: Database;

    /// The renderer used for `SELECT` operations.
    type Select: SelectRenderer<Self>;
    /// The renderer used for `INSERT` operations.
    type Insert: InsertRenderer<Self>;
    /// The renderer used for `UPDATE` operations.
    type Update: UpdateRenderer<Self>;
    /// The renderer used for `DELETE` operations.
    type Delete: DeleteRenderer<Self>;
    /// The renderer used for `UPSERT` operations.
    type Upsert: UpsertRenderer<Self>;

    fn quote_ident(&self, name: &str) -> String;
}

/// Marks a dialect that supports `RETURNING`.
///
/// This gates the `exec_returning` methods on the DML builders, so a dialect
/// without `RETURNING` rejects them at compile time.
pub trait SupportsReturning: Dialect {}
/// Marks a dialect that supports `FULL [OUTER] JOIN`.
///
/// This gates [`crate::plan::join::Join::full`] at compile time. MySQL and MariaDB
/// do not implement it.
pub trait SupportsFullJoin: Dialect {}
/// Marks a dialect that supports full-text search.
pub trait SupportsFullTextSearch: Dialect {}
/// Marks a dialect that supports JSON values.
pub trait SupportsJson: Dialect {}
/// Marks a dialect that supports array values.
pub trait SupportsArrays: Dialect {}
/// Marks a dialect that supports upserts.
///
/// This gates [`crate::exec::db::Db::upsert`] and
/// [`crate::exec::db::Transaction::upsert`].
pub trait SupportsUpsert: Dialect {}

/// Rust types that can be treated as textual SQL expressions.
pub trait Textual {}
impl Textual for String {}
impl Textual for str {}
impl Textual for &str {}
impl Textual for std::borrow::Cow<'_, str> {}
