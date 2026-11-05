//! SQL dialect traits and generic spelling hooks.

use super::writer::SqlWriter;
use crate::plan::backend::Dialect;

/// The DML operation requesting returned rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturningOp {
    Insert,
    Update,
    Delete,
    Upsert,
}

/// Where a dialect places the clause that returns rows from a DML statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReturningPosition {
    /// After the complete statement, as in PostgreSQL and SQLite `RETURNING`.
    Trailing,
    /// For `INSERT`: after the target and column list, before `VALUES`.
    ///
    /// This is where SQL Server's `OUTPUT INSERTED.*` belongs.
    BeforeValues,
    /// For `UPDATE` and `DELETE`: before `WHERE`.
    ///
    /// This is where SQL Server's `OUTPUT` clause belongs.
    BeforeWhere,
}

/// Generic SQL spelling hooks used by the built-in renderers.
///
/// The neutral [`Dialect`] identifies the database and chooses renderers; this
/// trait adds the SQL spelling defaults that most SQL dialects share. A driver
/// implements it only when its syntax differs from the default.
///
/// The methods use [`SqlWriter`], which is available once `Self` is a
/// [`Dialect`].
pub trait SqlSyntax: Dialect + 'static + Sized + Send + Sync {
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
        writer.push(" RETURNING *");
    }

    /// Where the returned-row clause belongs for `op`.
    ///
    /// The default is [`ReturningPosition::Trailing`], which matches
    /// PostgreSQL and SQLite. SQL Server places `OUTPUT` before `VALUES` and
    /// before `WHERE`.
    fn returning_position(&self, _op: ReturningOp) -> ReturningPosition {
        ReturningPosition::Trailing
    }

    /// Writes the returned-row clause for `op`.
    ///
    /// The default keeps the existing `RETURNING *` spelling. Dialects whose
    /// spelling depends on the operation override this.
    fn write_returning_clause(&self, _op: ReturningOp, writer: &mut SqlWriter<Self>)
    where
        Self: Dialect,
    {
        Self::write_returning(writer);
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
        writer.push(" ");
        writer.push(operator);
        writer.push(" ");
    }

    /// Writes the `NOT` keyword without an opening parenthesis.
    ///
    /// [`crate::plan::expr::ExprKind::Not`] writes the parentheses around the operand,
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

/// A [`Dialect`] whose query language is SQL and therefore uses [`SqlSyntax`]
/// spelling hooks.
///
/// A Cypher or document backend implements [`Dialect`] directly and supplies
/// its own renderers; it does not need to implement `SqlSyntax`.
pub trait SqlDialect: Dialect + SqlSyntax {}

impl<D: Dialect + SqlSyntax> SqlDialect for D {}

/// Rust types that can be treated as textual SQL expressions.
pub trait Textual {}
impl Textual for String {}
impl Textual for str {}
impl Textual for &str {}
impl Textual for std::borrow::Cow<'_, str> {}
