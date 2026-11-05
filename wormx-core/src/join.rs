//! Typed SQL joins and join sources.

use sqlx_core::error::Result;

use crate::dialect::{Dialect, Expr, SqlWriter};
use crate::metadata::TableDef;
use crate::query::Subquery;

/// A table or table alias used as a `FROM`/`JOIN` source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSource {
    pub def: &'static TableDef,
    pub alias: Option<&'static str>,
    /// Dialect-specific table hints, for example `"ROWLOCK, READPAST"`.
    ///
    /// The dialect decides the wrapper syntax. Dialects that do not support
    /// table hints render nothing for this field.
    pub hints: Option<&'static str>,
}

impl TableSource {
    /// Creates a table source with an optional alias and no hints.
    pub const fn new(def: &'static TableDef, alias: Option<&'static str>) -> Self {
        Self {
            def,
            alias,
            hints: None,
        }
    }

    /// Attaches raw table hints, for example `"ROWLOCK, READPAST"`, or a typed
    /// [`TableHint`].
    pub fn with_hint(mut self, hints: impl Into<&'static str>) -> Self {
        self.hints = Some(hints.into());
        self
    }

    /// Attaches a typed table hint.
    pub const fn with_table_hint(mut self, hint: TableHint) -> Self {
        self.hints = Some(hint.as_sql());
        self
    }

    /// Writes the table, alias, and any hints.
    pub fn render<D: Dialect>(&self, writer: &mut SqlWriter<'_, D>) {
        push_table(writer, self.def.schema, self.def.name);
        if let Some(alias) = self.alias {
            writer.write_as();
            writer.push_ident(alias);
        }
        if let Some(hints) = self.hints {
            writer.write_table_hint(hints);
        }
    }
}

/// A typed table hint for dialects that support them.
///
/// Use [`TableHint::Raw`] for hints not modeled here, or to combine several
/// hints in one string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableHint {
    NoLock,
    RowLock,
    ReadPast,
    UpdLock,
    HoldLock,
    TabLock,
    TabLockX,
    RepeatableRead,
    Serializable,
    Raw(&'static str),
}

impl TableHint {
    /// The SQL text of the hint, without a `WITH (...)` wrapper.
    pub const fn as_sql(self) -> &'static str {
        match self {
            Self::NoLock => "NOLOCK",
            Self::RowLock => "ROWLOCK",
            Self::ReadPast => "READPAST",
            Self::UpdLock => "UPDLOCK",
            Self::HoldLock => "HOLDLOCK",
            Self::TabLock => "TABLOCK",
            Self::TabLockX => "TABLOCKX",
            Self::RepeatableRead => "REPEATABLEREAD",
            Self::Serializable => "SERIALIZABLE",
            Self::Raw(sql) => sql,
        }
    }
}

impl From<TableHint> for &'static str {
    fn from(hint: TableHint) -> Self {
        hint.as_sql()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The SQL join kind.
pub enum JoinKind {
    /// `INNER JOIN`
    Inner,
    /// `LEFT JOIN`
    Left,
    /// `RIGHT JOIN`
    Right,
    /// `FULL JOIN`
    Full,
}

enum JoinSource<D: Dialect> {
    Table(TableSource),
    Derived {
        query: Box<dyn Subquery<D>>,
        alias: String,
    },
    Named(String),
}

/// A typed `JOIN` clause.
///
/// The right-hand side can be a generated table, a derived table (subquery),
/// or a named CTE.
pub struct Join<D: Dialect> {
    kind: JoinKind,
    source: JoinSource<D>,
    on: Expr<bool, D>,
}

impl<D: Dialect> Join<D> {
    pub fn inner(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Inner, source, on)
    }

    pub fn left(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Left, source, on)
    }

    pub fn right(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Right, source, on)
    }

    pub fn full(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Full, source, on)
    }

    pub fn inner_derived(
        query: impl Subquery<D> + 'static,
        alias: impl Into<String>,
        on: Expr<bool, D>,
    ) -> Self {
        Self::derived(JoinKind::Inner, query, alias, on)
    }

    pub fn left_derived(
        query: impl Subquery<D> + 'static,
        alias: impl Into<String>,
        on: Expr<bool, D>,
    ) -> Self {
        Self::derived(JoinKind::Left, query, alias, on)
    }

    pub fn inner_named(name: impl Into<String>, on: Expr<bool, D>) -> Self {
        Self {
            kind: JoinKind::Inner,
            source: JoinSource::Named(name.into()),
            on,
        }
    }

    pub fn left_named(name: impl Into<String>, on: Expr<bool, D>) -> Self {
        Self {
            kind: JoinKind::Left,
            source: JoinSource::Named(name.into()),
            on,
        }
    }

    fn table(kind: JoinKind, source: TableSource, on: Expr<bool, D>) -> Self {
        Self {
            kind,
            source: JoinSource::Table(source),
            on,
        }
    }

    fn derived(
        kind: JoinKind,
        query: impl Subquery<D> + 'static,
        alias: impl Into<String>,
        on: Expr<bool, D>,
    ) -> Self {
        Self {
            kind,
            source: JoinSource::Derived {
                query: Box::new(query),
                alias: alias.into(),
            },
            on,
        }
    }

    /// Writes the join clause.
    ///
    /// This is public so an external [`crate::query::SelectRenderer`] can
    /// reproduce the generic select renderer for joined queries.
    pub fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_join(self.kind);
        match &mut self.source {
            JoinSource::Table(source) => source.render(writer),
            JoinSource::Derived { query, alias } => {
                writer.write_group_start();
                query.render(writer)?;
                writer.write_group_end();
                writer.write_as();
                writer.push_ident(alias);
            }
            JoinSource::Named(name) => writer.push_ident(name),
        }
        writer.write_on();
        self.on.render(writer)
    }
}

pub(crate) fn push_table<D: Dialect>(
    writer: &mut SqlWriter<'_, D>,
    schema: Option<&str>,
    table: &str,
) {
    if let Some(schema) = schema {
        writer.push_ident(schema);
        writer.write_identifier_separator();
    }
    writer.push_ident(table);
}
