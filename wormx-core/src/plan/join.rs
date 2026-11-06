//! Typed joins and join sources.

use super::backend::{Dialect, SupportsFullJoin, SupportsNaturalJoin, SupportsUsing};
use super::expr::Expr;
use super::query::Subquery;
use crate::schema::TableDef;

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
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The join kind.
pub enum JoinKind {
    /// `INNER JOIN`
    Inner,
    /// `LEFT JOIN`
    Left,
    /// `RIGHT JOIN`
    Right,
    /// `FULL JOIN`
    Full,
    /// `CROSS JOIN`
    Cross,
}

/// How a join is constrained.
///
/// A join uses exactly one of these forms; `CROSS JOIN` is the only join without
/// an `ON`, `USING`, or `NATURAL` constraint.
#[non_exhaustive]
pub enum JoinConstraint<D: Dialect> {
    /// `ON <expr>`.
    On(Expr<bool, D>),
    /// `USING (<columns>)`.
    Using(&'static [&'static str]),
    /// `NATURAL`, which uses every column name shared by both sides.
    Natural,
    /// No constraint, for `CROSS JOIN`.
    None,
}

/// The right-hand side of a [`Join`].
///
/// This is public so a custom [`crate::plan::query::SelectRenderer`] can
/// inspect a join and render it in a backend-specific form instead of the
/// generic join spelling.
#[non_exhaustive]
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

impl<D: Dialect> JoinSource<D> {
    /// Creates a table source.
    pub fn table(source: TableSource) -> Self {
        Self::Table(source)
    }

    /// Creates a derived-table source.
    pub fn derived(query: impl Subquery<D> + 'static, alias: impl Into<String>) -> Self {
        Self::Derived {
            query: Box::new(query),
            alias: alias.into(),
        }
    }

    /// Creates a named-CTE source.
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }
}

impl<D: Dialect> From<TableSource> for JoinSource<D> {
    fn from(source: TableSource) -> Self {
        Self::Table(source)
    }
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
    /// The join constraint.
    pub constraint: JoinConstraint<D>,
}

impl<D: Dialect> Join<D> {
    /// Creates a join from an explicit kind, source, and constraint.
    pub fn new(
        kind: JoinKind,
        source: impl Into<JoinSource<D>>,
        constraint: JoinConstraint<D>,
    ) -> Self {
        Self {
            kind,
            source: source.into(),
            constraint,
        }
    }

    /// Returns the join kind.
    pub fn kind(&self) -> JoinKind {
        self.kind
    }

    /// Returns the join source.
    pub fn source(&self) -> &JoinSource<D> {
        &self.source
    }

    /// Returns the join constraint.
    pub fn constraint(&self) -> &JoinConstraint<D> {
        &self.constraint
    }

    /// Returns the `ON` predicate when this join uses one.
    pub fn on(&self) -> Option<&Expr<bool, D>> {
        match &self.constraint {
            JoinConstraint::On(expr) => Some(expr),
            _ => None,
        }
    }

    /// Creates an `INNER JOIN ... ON ...`.
    pub fn inner(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Inner, source, on)
    }

    /// Creates a `LEFT JOIN ... ON ...`.
    pub fn left(source: TableSource, on: Expr<bool, D>) -> Self {
        Self::table(JoinKind::Left, source, on)
    }

    /// Creates a `RIGHT JOIN ... ON ...`.
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

    /// Creates a `CROSS JOIN`.
    pub fn cross(source: impl Into<JoinSource<D>>) -> Self {
        Self::new(JoinKind::Cross, source, JoinConstraint::None)
    }

    /// Creates a join with a `USING (...)` constraint.
    ///
    /// Available only for dialects that support `USING`.
    pub fn using(
        kind: JoinKind,
        source: impl Into<JoinSource<D>>,
        columns: &'static [&'static str],
    ) -> Self
    where
        D: SupportsUsing,
    {
        Self::new(kind, source, JoinConstraint::Using(columns))
    }

    /// Creates a `NATURAL` join.
    ///
    /// Available only for dialects that support `NATURAL JOIN`.
    pub fn natural(kind: JoinKind, source: impl Into<JoinSource<D>>) -> Self
    where
        D: SupportsNaturalJoin,
    {
        Self::new(kind, source, JoinConstraint::Natural)
    }

    /// Creates an `INNER JOIN` over a derived table.
    pub fn inner_derived(
        query: impl Subquery<D> + 'static,
        alias: impl Into<String>,
        on: Expr<bool, D>,
    ) -> Self {
        Self::derived(JoinKind::Inner, query, alias, on)
    }

    /// Creates a `LEFT JOIN` over a derived table.
    pub fn left_derived(
        query: impl Subquery<D> + 'static,
        alias: impl Into<String>,
        on: Expr<bool, D>,
    ) -> Self {
        Self::derived(JoinKind::Left, query, alias, on)
    }

    /// Creates an `INNER JOIN` over a named CTE.
    pub fn inner_named(name: impl Into<String>, on: Expr<bool, D>) -> Self {
        Self::new(
            JoinKind::Inner,
            JoinSource::named(name),
            JoinConstraint::On(on),
        )
    }

    /// Creates a `LEFT JOIN` over a named CTE.
    pub fn left_named(name: impl Into<String>, on: Expr<bool, D>) -> Self {
        Self::new(
            JoinKind::Left,
            JoinSource::named(name),
            JoinConstraint::On(on),
        )
    }

    fn table(kind: JoinKind, source: TableSource, on: Expr<bool, D>) -> Self {
        Self::new(kind, source, JoinConstraint::On(on))
    }

    fn derived(
        kind: JoinKind,
        query: impl Subquery<D> + 'static,
        alias: impl Into<String>,
        on: Expr<bool, D>,
    ) -> Self {
        Self::new(
            kind,
            JoinSource::derived(query, alias),
            JoinConstraint::On(on),
        )
    }
}
