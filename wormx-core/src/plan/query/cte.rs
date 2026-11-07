use sqlx_core::error::Result;

use crate::plan::backend::Dialect;
use crate::plan::command::Writer;

/// A subquery that can be rendered inside an expression.
pub trait Subquery<D: Dialect>: Send {
    fn render(&mut self, writer: &mut Writer<D>) -> Result<()>;
}

/// A common table expression: a name bound to a query.
pub struct Cte<D: Dialect> {
    name: String,
    query: Box<dyn Subquery<D>>,
}

impl<D: Dialect> Cte<D> {
    /// Creates a common table expression.
    pub fn new(name: impl Into<String>, query: impl Subquery<D> + 'static) -> Self {
        Self {
            name: name.into(),
            query: Box::new(query),
        }
    }

    /// Returns the CTE name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the CTE query for rendering.
    ///
    /// SQL rendering lives in [`crate::sql::render`]; custom renderers can use
    /// this accessor to write their own CTE syntax.
    pub fn query_mut(&mut self) -> &mut dyn Subquery<D> {
        self.query.as_mut()
    }
}
