use std::borrow::Cow;

use itertools::Either;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{ColumnSelector, ComparisonTarget},
    mongodb::sanitize::safe_name,
};

/// Given a column target returns a MongoDB expression that resolves to the value of the
/// corresponding field, either in the target collection of a query request, or in the related
/// collection.
pub fn column_ref(column: &ComparisonTarget) -> Result<Cow<'_, str>, MongoAgentError> {
    let path = match column {
        ComparisonTarget::Column { name, path, .. } => {
            let relations_path = path.iter().map(AsRef::as_ref);
            let nested_object_path = column_selector_path(name);
            Either::Left(relations_path.chain(nested_object_path))
        }
        ComparisonTarget::RootCollectionColumn { name, .. } => {
            Either::Right(std::iter::once("$$ROOT").chain(column_selector_path(name)))
        }
    };
    safe_selector(path)
}

fn column_selector_path(selector: &ColumnSelector) -> impl Iterator<Item = &str> {
    match selector {
        ColumnSelector::Path(fields) => Either::Left(fields.iter().map(AsRef::as_ref)),
        ColumnSelector::Column(col_name) => Either::Right(std::iter::once(col_name.as_ref())),
    }
}

/// Given an iterable of fields to access, ensures that each field name does not include characters
/// that could be interpereted as a MongoDB expression.
fn safe_selector<'a>(
    path: impl IntoIterator<Item = &'a str>,
) -> Result<Cow<'a, str>, MongoAgentError> {
    let mut safe_elements = path
        .into_iter()
        .map(safe_name)
        .collect::<Result<Vec<Cow<str>>, MongoAgentError>>()?;
    if safe_elements.len() == 1 {
        Ok(safe_elements.pop().unwrap())
    } else {
        Ok(Cow::Owned(safe_elements.join(".")))
    }
}
