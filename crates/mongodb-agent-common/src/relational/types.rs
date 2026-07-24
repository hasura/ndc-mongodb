//! Types used in relational query processing.

use mongodb_support::aggregate::Pipeline;

use super::ColumnMapping;

/// Result of building a pipeline from a relation tree.
#[derive(Debug, Clone)]
pub struct RelationalPipelineResult {
    /// The logical name of the query source. For a physical collection this is the collection
    /// name; for a native query this is the native query name. Retained for logging, `$unionWith`
    /// composition, and existing test semantics.
    pub collection: String,
    /// The aggregation pipeline.
    pub pipeline: Pipeline,
    /// Column mapping for the output (index → field name).
    pub output_columns: ColumnMapping,
    /// The physical collection to run the aggregation against, if any.
    ///
    /// - `Some(name)` → execute `db.<name>.aggregate(pipeline)`. This is the physical collection
    ///   for a collection scan, or the `input_collection` of a native query.
    /// - `None` → execute a database-level `db.aggregate(pipeline)`. Used for native queries that
    ///   have no `input_collection` (e.g. pipelines that start with `$documents`).
    pub target_collection: Option<String>,
}
