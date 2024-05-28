use std::borrow::Cow;
use std::iter::once;

use itertools::Either;

use crate::{
    interface_types::MongoAgentError, mongo_query_plan::ComparisonTarget,
    mongodb::sanitize::safe_name,
};

/// Given a column target returns a MongoDB expression that resolves to the value of the
/// corresponding field, either in the target collection of a query request, or in the related
/// collection.
pub fn column_ref(column: &ComparisonTarget) -> Result<Cow<'_, str>, MongoAgentError> {
    let path = match column {
        ComparisonTarget::Column {
            name,
            field_path,
            path,
            ..
        } => Either::Left(
            path.iter()
                .chain(once(name))
                .chain(field_path.iter().flatten())
                .map(AsRef::as_ref),
        ),
        ComparisonTarget::RootCollectionColumn {
            name, field_path, ..
        } => Either::Right(
            once("$$ROOT")
                .chain(once(name.as_ref()))
                .chain(field_path.iter().flatten().map(AsRef::as_ref)),
        ),
    };
    safe_selector(path)
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
