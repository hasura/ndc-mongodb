use dc_api_types::comparison_column::ColumnSelector;

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
    column_name: &ColumnSelector,
    collection_name: Option<&str>,
) -> Result<String, MongoAgentError> {
    let reference = if let Some(collection) = collection_name {
        // This assumes that a related collection has been brought into scope by a $lookup stage.
        format!(
            "{}.{}",
            safe_name(collection)?,
            safe_column_selector(column_name)?
        )
    } else {
        format!("{}", safe_column_selector(column_name)?)
    };
    Ok(reference)
}
