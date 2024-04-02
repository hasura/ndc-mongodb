use dc_api_types::ComparisonColumn;

use crate::{
    interface_types::MongoAgentError,
    mongodb::sanitize::{safe_column_selector, safe_name},
};

/// Given a column, and an optional relationship name returns a MongoDB expression that
/// resolves to the value of the corresponding field, either in the target collection of a query
/// request, or in the related collection.
///
/// evaluating them as expressions.
pub fn column_ref(
    column: &ComparisonColumn,
    collection_name: Option<&str>,
) -> Result<String, MongoAgentError> {
    if column.path.as_ref().map(|path| !path.is_empty()).unwrap_or(false) {
        return Err(MongoAgentError::NotImplemented("comparisons against root query table columns")) 
    }

    let reference = if let Some(collection) = collection_name {
        // This assumes that a related collection has been brought into scope by a $lookup stage.
        format!(
            "{}.{}",
            safe_name(collection)?,
            safe_column_selector(&column.name)?
        )
    } else {
        format!("{}", safe_column_selector(&column.name)?)
    };
    Ok(reference)
}
