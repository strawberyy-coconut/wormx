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
}

impl TableSource {
    /// Creates a table source with an optional alias.
    pub const fn new(def: &'static TableDef, alias: Option<&'static str>) -> Self {
        Self { def, alias }
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

    pub(crate) fn render(&mut self, writer: &mut SqlWriter<'_, D>) -> Result<()> {
        writer.write_join(self.kind);
        match &mut self.source {
            JoinSource::Table(source) => push_source(writer, *source),
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

pub(crate) fn push_source<D: Dialect>(writer: &mut SqlWriter<'_, D>, source: TableSource) {
    push_table(writer, source.def.schema, source.def.name);
    if let Some(alias) = source.alias {
        writer.write_as();
        writer.push_ident(alias);
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
