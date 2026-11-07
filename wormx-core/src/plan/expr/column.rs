#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnRef {
    pub schema: Option<String>,
    pub table: String,
    pub alias: Option<String>,
    pub column: String,
}

impl ColumnRef {
    pub fn new(
        schema: Option<impl Into<String>>,
        table: impl Into<String>,
        column: impl Into<String>,
    ) -> Self {
        Self {
            schema: schema.map(Into::into),
            table: table.into(),
            alias: None,
            column: column.into(),
        }
    }

    pub fn with_alias(
        schema: Option<impl Into<String>>,
        table: impl Into<String>,
        alias: Option<impl Into<String>>,
        column: impl Into<String>,
    ) -> Self {
        Self {
            schema: schema.map(Into::into),
            table: table.into(),
            alias: alias.map(Into::into),
            column: column.into(),
        }
    }
}

/// Compatibility alias for the old name.
pub type ColumnExpr = ColumnRef;
