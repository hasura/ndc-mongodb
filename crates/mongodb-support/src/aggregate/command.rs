use mongodb::bson::Document;

use super::Pipeline;

/// Aggregate command used with, e.g., `db.<collection-name>.aggregate()`
///
/// This is not a complete implementation - only the fields needed by the connector are listed.
pub struct AggregateCommand {
    pub collection: Option<String>,
    pub pipeline: Pipeline,
    pub let_vars: Option<Document>,
}
