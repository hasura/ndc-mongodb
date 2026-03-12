//! Error types for relational query processing.

use thiserror::Error;

/// Errors that can occur during relational query processing.
#[derive(Debug, Error)]
pub enum RelationalError {
    /// The column index is out of bounds for the current column mapping.
    #[error("Invalid column index: {0}")]
    InvalidColumnIndex(u64),

    /// The relation type is not supported.
    #[error("Unsupported relation: {0}")]
    UnsupportedRelation(String),

    /// The expression type is not supported.
    #[error("Unsupported expression: {0}")]
    UnsupportedExpression(String),

    /// A From relation is required as the base of the query.
    #[error("No collection specified - query must start with a From relation")]
    NoCollection,

    /// Sort expression must be a column reference in Phase 1.
    #[error("Sort expression must be a column reference, got: {0}")]
    InvalidSortExpression(String),

    /// The join type is not supported.
    #[error("Unsupported join type: {0}")]
    UnsupportedJoinType(String),

    /// The right side of a join must be a From relation.
    #[error("Right side of join must be a From relation, got: {0}")]
    InvalidJoinRightSide(String),

    /// Invalid union operation.
    #[error("Invalid union: {0}")]
    InvalidUnion(String),

    /// Expression nesting is too deep.
    #[error("Expression nesting too deep (limit: 512)")]
    ExpressionTooDeep,
}
