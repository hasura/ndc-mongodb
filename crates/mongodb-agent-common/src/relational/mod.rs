//! Relational query support for MongoDB.
//!
//! This module implements the NDC relational query specification, translating
//! relational algebra operations to MongoDB aggregation pipelines.

mod column_mapping;
mod column_origin;
mod error;
mod execute;
pub mod expression;
mod normalize_joins;
mod optimize_filters;
mod pipeline_builder;
mod pushdown_predicates;
mod types;

pub use column_mapping::ColumnMapping;
pub use column_origin::{trace_column_origin, ColumnOrigin};
pub use error::RelationalError;
pub use execute::{execute_relational_query, execute_relational_query_stream};
pub use normalize_joins::{column_count, normalize_right_joins};
pub use optimize_filters::{extract_early_match, EarlyMatchResult};
pub use pipeline_builder::build_relational_pipeline;
pub use pushdown_predicates::pushdown_predicates;
pub use types::RelationalPipelineResult;

#[cfg(test)]
mod tests;
