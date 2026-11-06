//! The `RETURNING` clause wrapper shared by the DML builders.

/// A DML builder that has opted into `RETURNING`.
///
/// Call `.returning()` on a DML builder to render the dialect's returned-row
/// clause (`RETURNING *`, `OUTPUT INSERTED.*`, or `OUTPUT DELETED.*`) and decode
/// the affected rows. `exec` and `exec_in` then return the decoded row or rows
/// instead of the driver's `QueryResult`.
///
/// ```ignore
/// let updated = db
///     .update::<Users>()
///     .set(Users::fields().active(), true)
///     .filter(Users::fields().id().eq(1_i64))
///     .returning()
///     .exec()
///     .await?;
/// ```
pub struct Returning<B> {
    pub(crate) inner: B,
}

impl<B> Returning<B> {
    pub(crate) fn new(inner: B) -> Self {
        Self { inner }
    }
}
