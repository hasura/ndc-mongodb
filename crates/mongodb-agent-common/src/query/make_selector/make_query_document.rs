use anyhow::anyhow;
use itertools::Itertools as _;
use mongodb::bson::{self, doc, Bson};
use ndc_models::UnaryComparisonOperator;

use crate::{
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{
        ArrayComparison, ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type,
    },
    query::{column_ref::ColumnRef, serialization::json_to_bson},
};

use super::Result;

#[derive(Clone, Debug)]
pub struct QueryDocument(pub bson::Document);

impl QueryDocument {
    pub fn into_document(self) -> bson::Document {
        self.0
    }
}

/// Translates the given expression into a query document for use in a $match aggregation stage if
/// possible. If the expression cannot be expressed as a query document returns `Ok(None)`.
pub fn make_query_document(expr: &Expression) -> Result<Option<QueryDocument>> {
    match expr {
        Expression::And { expressions } => {
            let sub_exps: Option<Vec<_>> = expressions
                .clone()
                .iter()
                .map(make_query_document)
                .collect::<Result<_>>()?;
            // If any of the sub expressions are not query documents then we have to back-track
            // and map everything to aggregation expressions.
            let plan = sub_exps.map(|exps| {
                QueryDocument(
                    doc! { "$and": exps.into_iter().map(QueryDocument::into_document).collect_vec() },
                )
            });
            Ok(plan)
        }
        Expression::Or { expressions } => {
            let sub_exps: Option<Vec<QueryDocument>> = expressions
                .clone()
                .iter()
                .map(make_query_document)
                .collect::<Result<_>>()?;
            let plan = sub_exps.map(|exps| {
                QueryDocument(
                    doc! { "$or": exps.into_iter().map(QueryDocument::into_document).collect_vec() },
                )
            });
            Ok(plan)
        }
        Expression::Not { expression } => {
            let sub_expression = make_query_document(expression)?;
            let plan =
                sub_expression.map(|expr| QueryDocument(doc! { "$nor": [expr.into_document()] }));
            Ok(plan)
        }
        Expression::Exists {
            in_collection,
            predicate,
        } => make_query_document_for_exists(in_collection, predicate.as_deref()),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => make_binary_comparison_selector(column, operator, value),
        Expression::UnaryComparisonOperator { column, operator } => {
            make_unary_comparison_selector(column, operator)
        }
        Expression::ArrayComparison { column, comparison } => {
            make_array_comparison_selector(column, comparison)
        }
    }
}

// TODO: ENG-1148 Move predicate application to the join step instead of filtering the entire
// related or unrelated collection here
fn make_query_document_for_exists(
    in_collection: &ExistsInCollection,
    predicate: Option<&Expression>,
) -> Result<Option<QueryDocument>> {
    let plan = match (in_collection, predicate) {
        (ExistsInCollection::Related { relationship }, Some(predicate)) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            exists_in_array(relationship_ref, predicate)?
        }
        (ExistsInCollection::Related { relationship }, None) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            exists_in_array_no_predicate(relationship_ref)
        }
        // Unrelated collection references cannot be expressed in a query document due to
        // a requirement to reference a pipeline variable.
        (ExistsInCollection::Unrelated { .. }, _) => None,
        (
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            },
            Some(predicate),
        ) => {
            let column_ref = ColumnRef::from_column_and_field_path(column_name, Some(field_path));
            exists_in_array(column_ref, predicate)?
        }
        (
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            },
            None,
        ) => {
            let column_ref = ColumnRef::from_column_and_field_path(column_name, Some(field_path));
            exists_in_array_no_predicate(column_ref)
        }
        (
            ExistsInCollection::NestedScalarCollection {
                column_name,
                field_path,
                ..
            },
            Some(predicate),
        ) => {
            let column_ref = ColumnRef::from_column_and_field_path(column_name, Some(field_path));
            exists_in_array(column_ref, predicate)? // TODO: predicate expects objects with a __value field
        }
        (
            ExistsInCollection::NestedScalarCollection {
                column_name,
                field_path,
                ..
            },
            None,
        ) => {
            let column_ref = ColumnRef::from_column_and_field_path(column_name, Some(field_path));
            exists_in_array_no_predicate(column_ref)
        }
    };
    Ok(plan)
}

fn exists_in_array(
    array_ref: ColumnRef<'_>,
    predicate: &Expression,
) -> Result<Option<QueryDocument>> {
    let sub_expression = make_query_document(predicate)?;
    let plan = match (array_ref, sub_expression) {
        (ColumnRef::MatchKey(key), Some(QueryDocument(query_doc))) => Some(QueryDocument(doc! {
            key: { "$elemMatch": query_doc }
        })),
        _ => None,
    };
    Ok(plan)
}

fn exists_in_array_no_predicate(array_ref: ColumnRef<'_>) -> Option<QueryDocument> {
    match array_ref {
        ColumnRef::MatchKey(key) => Some(QueryDocument(doc! {
            key: {
                "$exists": true,
                "$not": { "$size": 0 },
            }
        })),
        _ => None,
    }
}

fn make_binary_comparison_selector(
    target_column: &ComparisonTarget,
    operator: &ComparisonFunction,
    value: &ComparisonValue,
) -> Result<Option<QueryDocument>> {
    let selector =
        value_expression(value)?.and_then(|value| {
            match ColumnRef::from_comparison_target(target_column) {
                ColumnRef::MatchKey(key) => {
                    Some(QueryDocument(operator.mongodb_match_query(key, value)))
                }
                _ => None,
            }
        });
    Ok(selector)
}

fn make_unary_comparison_selector(
    target_column: &ComparisonTarget,
    operator: &UnaryComparisonOperator,
) -> Result<Option<QueryDocument>> {
    let query_doc = match operator {
        UnaryComparisonOperator::IsNull => match ColumnRef::from_comparison_target(target_column) {
            ColumnRef::MatchKey(key) => Some(QueryDocument(doc! {
                key: { "$eq": null }
            })),
            _ => None,
        },
    };
    Ok(query_doc)
}

fn make_array_comparison_selector(
    column: &ComparisonTarget,
    comparison: &ArrayComparison,
) -> Result<Option<QueryDocument>> {
    let column_ref = ColumnRef::from_comparison_target(column);
    let ColumnRef::MatchKey(key) = column_ref else {
        return Ok(None);
    };
    let doc = match comparison {
        ArrayComparison::Contains { value } => value_expression(value)?.map(|value| {
            doc! {
                key: { "$elemMatch": { "$eq": value } }
            }
        }),
        ArrayComparison::IsEmpty => Some(doc! {
            key: { "$size": 0 }
        }),
    };
    Ok(doc.map(QueryDocument))
}

/// Only scalar comparison values can be represented in query documents. This function returns such
/// a representation if there is a legal way to do so.
fn value_expression(value: &ComparisonValue) -> Result<Option<Bson>> {
    let expression = match value {
        ComparisonValue::Scalar { value, value_type } => {
            let bson_value = bson_from_scalar_value(value, value_type)?;
            Some(bson_value)
        }
        ComparisonValue::Column { .. } => None,
        // Variables cannot be referenced in match documents
        ComparisonValue::Variable { .. } => None,
    };
    Ok(expression)
}

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}
