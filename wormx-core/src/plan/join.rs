//! Typed SQL joins and join sources.

use sqlx_core::error::Result;

use super::query::Subquery;
use crate::schema::TableDef;
use crate::sql::{Dialect, Expr, SqlWriter, SupportsFullJoin};

/// A table or table alias used as a `FROM`/`JOIN` source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TableSource {
    pub def: &'static TableDef,
    pub alias: Option<&'static str>,
    /// An opaque, dialect-defined suffix for the source.
    ///
    /// Core does not assign meaning to this text. A dialect that supports a
    /// source suffix renders it through [`crate::sql::SqlSyntax::write_source_suffix`]; the
    /// default implementation ignores it.
    pub suffix: Option<&'static str>,
}

impl TableSource {
    /// Creates a table source with an optional alias and no suffix.
    pub const fn new(def: &'static TableDef, alias: Option<&'static str>) -> Self {
        Self {
            def,
            alias,
            suffix: None,
        }
    }

    /// Attaches an opaque, dialect-defined source suffix.
    pub const fn with_suffix(mut self, suffix: &'static str) -> Self {
        self.suffix = Some(suffix);
        self
    }

    /// Writes the table, alias, and any dialect-defined suffix.
    pub fn render<D: Dialect>(&self, writer: &mut SqlWriter<'_, D>) {
        push_table(writer, self.def.schema, self.def.name);
        if let Some(alias) = self.alias {
            writer.write_as();
            writer.push_ident(alias);
        }
        if let Some(suffix) = self.suffix {
            writer.write_source_suffix(suffix);
        }
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

/// The right-hand side of a [`Join`].
///
/// This is public so a custom [`crate::plan::query::SelectRenderer`] can
/// inspect a join and render it in a backend-specific form instead of the
/// generic `JOIN <source> ON <expr>`.
pub enum JoinSource<D: Dialect> {
    /// A table or table alias.
    Table(TableSource),
    /// A derived table (subquery) with an alias.
    Derived {
        /// The subquery.
        query: Box<dyn Subquery<D>>,
        /// The derived table's alias.
        alias: String,
    },
    /// A named CTE.
    Named(String),
}

/// A typed `JOIN` clause.
///
/// The right-hand side can be a generated table, a derived table (subquery),
/// or a named CTE. The fields are public so renderers for other query
/// languages can map a join onto their own constructs.
pub struct Join<D: Dialect> {
    /// The join kind.
    pub kind: JoinKind,
    /// The right-hand side source.
    pub source: JoinSource<D>,
    /// The join predicate.
    pub on: Expr<bool, D>,
}

impl<D: Dialect> Join<D> {
    /// Returns the join kind.
    pub fn kind(&self) -> JoinKind {
        self.kind
    }

    /// Returns the join source.
    pub fn source(&self) -> &JoinSource<D> {
        &self.source
    }

    /// Returns the join predicate.
    pub fn on(&self) -> &Expr<bool, D> {
        &self.on
    }

    pub fn inner(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Inner, source, on)
    }

    pub fn left(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Left, source, on)
    }

    pub fn right(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Right, source, on)
    }

    /// Creates a `FULL JOIN`, available only for dialects that support it.
    pub fn full(source: TableSource, on: Expr<bool, D>) -> Self
    where
        D: SupportsFullJoin,
    {
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
    /// This is public so an external [`super::query::SelectRenderer`] can
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
