use std::{borrow::Cow, iter::once};

use itertools::Either;
use mongodb::bson::Bson;

use crate::{
    interface_types::MongoAgentError, mongo_query_plan::ComparisonTarget,
    mongodb::sanitize::safe_name,
};

pub type Result<T> = std::result::Result<T, MongoAgentError>;

/// Reference to a column / document field based on a [ComparisonTarget]. There are two contexts
/// where we reference columns:
///
/// - match queries, where the reference is a key in the document used in a `$match` aggregation stage
/// - aggregation expressions which appear in a number of contexts
///
/// Those two contexts are not compatible. For example in aggregation expressions column names are
/// prefixed with a dollar sign ($), but in match queries names are not prefixed. Expressions may
/// reference variables, while match queries may not. Some [ComparisonTarget] values **cannot** be
/// expressed in match queries. Those include root collection column references (which require
/// a variable reference), and columns with names that include characters that MongoDB evaluates
/// specially, such as dollar signs or dots.
///
/// This type provides a helper that attempts to produce a match query reference for
/// a [ComparisonTarget], but falls back to an aggregation expression if necessary. It is up to the
/// caller to switch contexts in the second case.
#[derive(Clone, Debug, PartialEq)]
pub enum ColumnRef<'a> {
    MatchKey(Cow<'a, str>),
    Expression(Bson),
}

impl<'a> ColumnRef<'a> {
    /// Given a column target returns a string that can be used in a MongoDB match query that
    /// references the corresponding field, either in the target collection of a query request, or in
    /// the related collection. Resolves nested fields, but does not traverse relationships.
    ///
    /// If the given target cannot be represented as a match query key, falls back to providing an
    /// aggregation expression referencing the column.
    pub fn from_comparison_target(column: &ComparisonTarget) -> ColumnRef<'_> {
        if let Ok(match_key) = column_match_key(column) {
            ColumnRef::MatchKey(match_key)
        } else {
            ColumnRef::Expression(column_expression(column))
        }
    }
}

/// Given a column target returns a string that can be used in a MongoDB match query that
/// references the corresponding field, either in the target collection of a query request, or in
/// the related collection. Resolves nested fields, but does not traverse relationships.
///
/// The string produced by this function cannot be used as an aggregation expression, only as
/// a match query key (a key in the document used in a `$match` stage).
fn column_match_key(column: &ComparisonTarget) -> Result<Cow<'_, str>> {
    let path = match column {
        ComparisonTarget::Column {
            name,
            field_path,
            // path,
            ..
        } => Either::Left(
            once(name)
                .chain(field_path.iter().flatten())
                .map(AsRef::as_ref),
        ),
        ComparisonTarget::RootCollectionColumn {
            name, field_path, ..
        } => Either::Right(
            // TODO: This doesn't work - we can't use a variable as the key in a match query
            once("$$ROOT")
                .chain(once(name.as_ref()))
                .chain(field_path.iter().flatten().map(AsRef::as_ref)),
        ),
    };
    safe_selector(path)
}

/// Given an iterable of fields to access, ensures that each field name does not include characters
/// that could be interpereted as a MongoDB expression.
fn safe_selector<'a>(path: impl IntoIterator<Item = &'a str>) -> Result<Cow<'a, str>> {
    let mut safe_elements = path
        .into_iter()
        .map(safe_name)
        .collect::<Result<Vec<Cow<str>>>>()?;
    if safe_elements.len() == 1 {
        Ok(safe_elements.pop().unwrap())
    } else {
        Ok(Cow::Owned(safe_elements.join(".")))
    }
}

/// Produces an aggregation expression that evaluates to the value of a given document field.
/// Unlike `column_ref` this expression cannot be used as a match query key - it can only be used
/// as an expression.
fn column_expression(column: &ComparisonTarget) -> Bson {
    Ok(format!("${}", column_ref(column)?))
}
