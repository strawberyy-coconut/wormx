//! SQL spelling helpers for the neutral command writer.

use super::syntax::{ReturningOp, ReturningPosition, SqlDialect};
pub use crate::plan::command::Writer;

/// Compatibility alias for the old name.
pub type SqlWriter<D> = Writer<D>;

impl<D: SqlDialect> Writer<D> {
    pub fn push_limit_offset(&mut self, has_order: bool, limit: Option<u64>, offset: Option<u64>) {
        let mut suffix = String::new();
        self.dialect()
            .write_limit_offset(has_order, limit, offset, &mut suffix);
        self.push(&suffix);
    }

    pub fn write_select(&mut self, distinct: bool) {
        D::write_select(distinct, self);
    }

    pub fn write_from(&mut self) {
        D::write_from(self);
    }

    pub fn write_where(&mut self) {
        D::write_where(self);
    }

    pub fn write_insert(&mut self) {
        D::write_insert(self);
    }

    pub fn write_insert_default(&mut self) {
        D::write_insert_default(self);
    }

    pub fn write_values(&mut self) {
        D::write_values(self);
    }

    pub fn write_update(&mut self) {
        D::write_update(self);
    }

    pub fn write_set(&mut self) {
        D::write_set(self);
    }

    pub fn write_equals(&mut self) {
        D::write_equals(self);
    }

    pub fn write_delete(&mut self) {
        D::write_delete(self);
    }

    pub fn write_returning(&mut self) {
        D::write_returning(self);
    }

    pub fn returning_position(&self, op: ReturningOp) -> ReturningPosition {
        self.dialect().returning_position(op)
    }

    pub fn write_returning_clause(&mut self, op: ReturningOp) {
        let dialect = self.dialect().clone();
        dialect.write_returning_clause(op, self);
    }

    pub fn write_input_column(&mut self, column: &str) {
        D::write_input_column(column, self);
    }

    pub fn write_on_conflict(&mut self) {
        D::write_on_conflict(self);
    }

    pub fn write_do_update_set(&mut self) {
        D::write_do_update_set(self);
    }

    pub fn write_and(&mut self) {
        D::write_and(self);
    }

    pub fn write_or(&mut self) {
        D::write_or(self);
    }

    pub fn write_join(&mut self, kind: crate::plan::join::JoinKind) {
        D::write_join(kind, self);
    }

    pub fn write_on(&mut self) {
        D::write_on(self);
    }

    pub fn write_group_by(&mut self) {
        D::write_group_by(self);
    }

    pub fn write_partition_by(&mut self) {
        D::write_partition_by(self);
    }

    pub fn write_over(&mut self) {
        D::write_over(self);
    }

    pub fn write_having(&mut self) {
        D::write_having(self);
    }

    pub fn write_order_by(&mut self) {
        D::write_order_by(self);
    }

    pub fn write_asc(&mut self) {
        D::write_asc(self);
    }

    pub fn write_desc(&mut self) {
        D::write_desc(self);
    }

    pub fn write_as(&mut self) {
        D::write_as(self);
    }

    pub fn write_comma(&mut self) {
        D::write_comma(self);
    }

    pub fn write_star(&mut self) {
        D::write_star(self);
    }

    pub fn write_count_star(&mut self) {
        D::write_count_star(self);
    }

    pub fn write_count_start(&mut self, distinct: bool) {
        D::write_count_start(distinct, self);
    }

    pub fn write_function_start(&mut self, name: &str) {
        D::write_function_start(name, self);
    }

    pub fn write_source_suffix(&mut self, suffix: &str) {
        let dialect = self.dialect().clone();
        dialect.write_source_suffix(suffix, self);
    }

    pub fn write_tail(&mut self, tail: &str) {
        let dialect = self.dialect().clone();
        dialect.write_tail(tail, self);
    }

    pub fn write_operator(&mut self, operator: &str) {
        D::write_operator(operator, self);
    }

    pub fn write_not(&mut self) {
        D::write_not(self);
    }

    pub fn write_in(&mut self, negated: bool) {
        D::write_in(negated, self);
    }

    pub fn write_is_null(&mut self, negated: bool) {
        D::write_is_null(negated, self);
    }

    pub fn write_like(&mut self) {
        D::write_like(self);
    }

    pub fn write_boolean_literal(&mut self, value: bool) {
        D::write_boolean_literal(value, self);
    }

    pub fn write_group_start(&mut self) {
        D::write_group_start(self);
    }

    pub fn write_group_end(&mut self) {
        D::write_group_end(self);
    }

    pub fn write_identifier_separator(&mut self) {
        D::write_identifier_separator(self);
    }

    pub fn write_space(&mut self) {
        D::write_space(self);
    }
}
