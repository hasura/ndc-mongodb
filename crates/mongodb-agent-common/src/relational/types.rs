//! Types used in relational query processing.

use mongodb_support::aggregate::Pipeline;

use super::ColumnMapping;

/// Result of building a pipeline from a relation tree.
#[derive(Debug, Clone)]
pub struct RelationalPipelineResult {
    /// The collection to query.
    pub collection: String,
    /// The aggregation pipeline.
    pub pipeline: Pipeline,
    /// Column mapping for the output (index → field name).
    pub output_columns: ColumnMapping,
}
