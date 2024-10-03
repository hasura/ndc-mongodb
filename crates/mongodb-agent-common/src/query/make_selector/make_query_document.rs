use std::iter::once;

use anyhow::anyhow;
use itertools::Itertools as _;
use mongodb::bson::{self, doc};
use ndc_models::UnaryComparisonOperator;

use crate::{
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type},
    query::{column_ref::ColumnRef, serialization::json_to_bson},
};

use super::Result;

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
            let column_ref = ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
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
            let column_ref = ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
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
    let query_doc = match value {
        ComparisonValue::Column {
            column: value_column,
        } => {
            // TODO: Do we want an implicit exists in the value relationship? If both target and
            // value reference relationships do we want an exists in a Cartesian product of the
            // two?
            if !value_column.relationship_path().is_empty() {
                return Err(MongoAgentError::NotImplemented("binary comparisons where the right-side of the comparison references a relationship".into()));
            }

            let left_operand = ColumnRef::from_comparison_target(target_column);
            let right_operand = ColumnRef::from_comparison_target(value_column);
            match (left_operand, right_operand) {
                (ColumnRef::MatchKey(left), ColumnRef::MatchKey(right)) => Some(QueryDocument(
                    operator.mongodb_match_query(left, right.into_owned().into()),
                )),
                _ => None,
            }
        }
        ComparisonValue::Scalar { value, value_type } => {
            let comparison_value = bson_from_scalar_value(value, value_type)?;
            match ColumnRef::from_comparison_target(target_column) {
                ColumnRef::MatchKey(key) => Some(QueryDocument(
                    operator.mongodb_match_query(key, comparison_value),
                )),
                _ => None,
            }
        }
        // Variables cannot be referenced in match documents
        ComparisonValue::Variable { .. } => None,
    };

    let implicit_exists_over_relationship =
        query_doc.map(|d| traverse_relationship_path(target_column.relationship_path(), d));

    Ok(implicit_exists_over_relationship)
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

    let implicit_exists_over_relationship =
        query_doc.map(|d| traverse_relationship_path(target_column.relationship_path(), d));

    Ok(implicit_exists_over_relationship)
}

/// For simple cases the target of an expression is a field reference. But if the target is
/// a column of a related collection then we're implicitly making an array comparison (because
/// related documents always come as an array, even for object relationships), so we have to wrap
/// the starting expression with an `$elemMatch` for each relationship that is traversed to reach
/// the target column.
fn traverse_relationship_path(
    path: &[ndc_models::RelationshipName],
    QueryDocument(mut expression): QueryDocument,
) -> QueryDocument {
    for path_element in path.iter().rev() {
        expression = doc! {
            path_element.to_string(): {
                "$elemMatch": expression
            }
        }
    }
    QueryDocument(expression)
}

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}
