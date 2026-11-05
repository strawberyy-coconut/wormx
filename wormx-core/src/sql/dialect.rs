//! Backwards-compatible facade over [`super::syntax`], [`super::writer`], and
//! [`super::expr`].
//!
//! New code can use those modules directly. This module keeps
//! `wormx_core::dialect` and `wormx_core::sql::dialect` working.

pub use super::expr::*;
pub use super::syntax::*;
pub use super::writer::*;
