//! The query text and bound-argument command writer.
//!
//! This is the language-neutral part of the writer: it accumulates a query
//! string and the database's bound arguments. SQL spelling helpers live in
//! [`crate::sql::writer`] and are only available for [`crate::sql::SqlDialect`].

use sqlx_core::arguments::Arguments;
use sqlx_core::database::Database;
use sqlx_core::encode::Encode;
use sqlx_core::error::Error;
use sqlx_core::types::Type;

use super::backend::Dialect;

/// A writer for query text plus bound arguments.
pub struct Writer<D: Dialect> {
    text: String,
    arguments: <D::DB as Database>::Arguments,
    dialect: D,
}

impl<D: Dialect> Writer<D> {
    pub fn new(dialect: D) -> Self {
        Self {
            text: String::new(),
            arguments: <D::DB as Database>::Arguments::default(),
            dialect,
        }
    }

    /// Creates a writer from a borrowed dialect by cloning it.
    pub fn from_ref(dialect: &D) -> Self
    where
        D: Clone,
    {
        Self::new(dialect.clone())
    }

    pub fn push(&mut self, text: &str) {
        self.text.push_str(text);
    }

    pub fn push_ident(&mut self, ident: &str) {
        let quoted = self.dialect.quote_ident(ident);
        self.text.push_str(&quoted);
    }

    pub fn bind<T>(&mut self, value: T) -> Result<(), Error>
    where
        T: for<'q> Encode<'q, D::DB> + Type<D::DB>,
    {
        self.arguments.add(value).map_err(Error::Encode)?;
        self.arguments
            .format_placeholder(&mut self.text)
            .map_err(|error| Error::Encode(Box::new(error)))?;
        Ok(())
    }

    pub fn finish(self) -> (String, <D::DB as Database>::Arguments) {
        (self.text, self.arguments)
    }

    pub fn dialect(&self) -> &D {
        &self.dialect
    }
}
