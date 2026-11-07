use sqlx_core::error::Result;

use crate::plan::query::Cte;
use crate::sql::{SqlDialect, Writer};

impl<D: SqlDialect> Cte<D> {
    /// Writes the CTE using SQL's `name AS (query)` spelling.
    pub fn render(&mut self, writer: &mut Writer<D>) -> Result<()> {
        writer.push_ident(self.name());
        writer.push(" AS (");
        self.query_mut().render(writer)?;
        writer.push(")");
        Ok(())
    }
}

/// Writes a `WITH` clause.
pub fn write_ctes<D: SqlDialect>(writer: &mut Writer<D>, ctes: &mut [Cte<D>]) -> Result<()> {
    if ctes.is_empty() {
        return Ok(());
    }

    writer.push("WITH ");
    for (index, cte) in ctes.iter_mut().enumerate() {
        if index > 0 {
            writer.write_comma();
        }
        cte.render(writer)?;
    }
    writer.write_space();
    Ok(())
}
