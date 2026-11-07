use wormx_core::{Delete, ProjectedSelect, Projection, Select, Table, TableSource, Update, Upsert};

use super::dialect::MssqlDialect;

/// A typed SQL Server table hint.
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

/// Converts a raw string or [`TableHint`] into SQL Server hint text.
pub trait IntoTableHint {
    fn into_table_hint(self) -> &'static str;
}

impl IntoTableHint for &'static str {
    fn into_table_hint(self) -> &'static str {
        self
    }
}

impl IntoTableHint for TableHint {
    fn into_table_hint(self) -> &'static str {
        self.as_sql()
    }
}

/// SQL Server table-hint builders.
///
/// The core `wormx` builders only know about opaque source suffixes. This trait
/// gives them SQL Server vocabulary without putting `ROWLOCK`, `READPAST`, or
/// the `WITH (...)` syntax in core.
pub trait MssqlTableExt: Sized {
    /// Attaches a SQL Server table hint, for example `TableHint::RowLock` or
    /// `"ROWLOCK, READPAST"`.
    fn hint<H: IntoTableHint>(self, hint: H) -> Self;
}

impl MssqlTableExt for TableSource {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Select<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T, P> MssqlTableExt for ProjectedSelect<MssqlDialect, T, P>
where
    T: Table,
    P: Projection<MssqlDialect>,
{
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Update<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Delete<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

impl<T: Table> MssqlTableExt for Upsert<MssqlDialect, T> {
    fn hint<H: IntoTableHint>(self, hint: H) -> Self {
        self.with_source_suffix(hint.into_table_hint())
    }
}

/// SQL Server statement-tail builders.
pub trait MssqlStatementExt: Sized {
    /// Sets the body of a SQL Server `OPTION (...)` clause, for example
    /// `"RECOMPILE"`.
    fn option(self, options: &'static str) -> Self;
}

impl<T: Table> MssqlStatementExt for Select<MssqlDialect, T> {
    fn option(self, options: &'static str) -> Self {
        self.with_tail(options)
    }
}

impl<T, P> MssqlStatementExt for ProjectedSelect<MssqlDialect, T, P>
where
    T: Table,
    P: Projection<MssqlDialect>,
{
    fn option(self, options: &'static str) -> Self {
        self.with_tail(options)
    }
}
