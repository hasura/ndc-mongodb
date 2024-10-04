use std::iter::once;

use anyhow::anyhow;
use itertools::Itertools as _;
use mongodb::bson::{self, doc, Bson};
use ndc_models::UnaryComparisonOperator;

use crate::{
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type},
    query::{
        column_ref::{column_expression, ColumnRef},
        query_variable_name::query_variable_name,
        serialization::json_to_bson,
    },
};

use super::Result;

#[derive(Clone, Debug)]
pub struct AggregationExpression(pub Bson);

impl AggregationExpression {
    fn into_bson(self) -> Bson {
        self.0
    }
}

pub fn make_aggregation_expression(expr: &Expression) -> Result<AggregationExpression> {
    match expr {
        Expression::And { expressions } => {
            let sub_exps: Vec<_> = expressions
                .clone()
                .iter()
                .map(make_aggregation_expression)
                .collect::<Result<_>>()?;
            let plan = AggregationExpression(
                doc! {
                    "$and": sub_exps.into_iter().map(AggregationExpression::into_bson).collect_vec()
                }
                .into(),
            );
            Ok(plan)
        }
        Expression::Or { expressions } => {
            let sub_exps: Vec<_> = expressions
                .clone()
                .iter()
                .map(make_aggregation_expression)
                .collect::<Result<_>>()?;
            let plan = AggregationExpression(
                doc! {
                    "$or": sub_exps.into_iter().map(AggregationExpression::into_bson).collect_vec()
                }
                .into(),
            );
            Ok(plan)
        }
        Expression::Not { expression } => {
            let sub_expression = make_aggregation_expression(expression)?;
            let plan = AggregationExpression(doc! { "$nor": [sub_expression.into_bson()] }.into());
            Ok(plan)
        }
        Expression::Exists {
            in_collection,
            predicate,
        } => make_aggregation_expression_for_exists(in_collection, predicate.as_deref()),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => make_binary_comparison_selector(column, operator, value),
        Expression::UnaryComparisonOperator { column, operator } => {
            make_unary_comparison_selector(column, *operator)
        }
    }
}

// TODO: ENG-1148 Move predicate application to the join step instead of filtering the entire
// related or unrelated collection here
pub fn make_aggregation_expression_for_exists(
    in_collection: &ExistsInCollection,
    predicate: Option<&Expression>,
) -> Result<AggregationExpression> {
    let expression = match (in_collection, predicate) {
        (ExistsInCollection::Related { relationship }, Some(predicate)) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            exists_in_array(relationship_ref, predicate)?
        }
        (ExistsInCollection::Related { relationship }, None) => {
            let relationship_ref = ColumnRef::from_relationship(relationship);
            exists_in_array_no_predicate(relationship_ref)
        }
        (
            ExistsInCollection::Unrelated {
                unrelated_collection,
            },
            Some(predicate),
        ) => {
            let collection_ref = ColumnRef::from_unrelated_collection(unrelated_collection);
            exists_in_array(collection_ref, predicate)?
        }
        (
            ExistsInCollection::Unrelated {
                unrelated_collection,
            },
            None,
        ) => {
            let collection_ref = ColumnRef::from_unrelated_collection(unrelated_collection);
            exists_in_array_no_predicate(collection_ref)
        }
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
    Ok(expression)
}

fn exists_in_array(
    array_ref: ColumnRef<'_>,
    predicate: &Expression,
) -> Result<AggregationExpression> {
    let AggregationExpression(sub_expression) = make_aggregation_expression(predicate)?;
    Ok(AggregationExpression(
        doc! {
            "$anyElementTrue": {
                "$map": {
                    "input": array_ref.into_aggregate_expression(),
                    "as": "CURRENT", // implicitly changes the document root in `exp` to be the array element
                    "in": sub_expression,
                }
            }
        }
        .into(),
    ))
}

fn exists_in_array_no_predicate(array_ref: ColumnRef<'_>) -> AggregationExpression {
    let index_zero = "0".into();
    let first_element_ref = array_ref.into_nested_field(&index_zero);
    AggregationExpression(
        doc! {
            "$ne": [first_element_ref.into_aggregate_expression(), null]
        }
        .into(),
    )
}

fn make_binary_comparison_selector(
    target_column: &ComparisonTarget,
    operator: &ComparisonFunction,
    value: &ComparisonValue,
) -> Result<AggregationExpression> {
    let aggregation_expression = match value {
        ComparisonValue::Column {
            column: value_column,
        } => {
            // TODO: ENG-1153 Do we want an implicit exists in the value relationship? If both
            // target and value reference relationships do we want an exists in a Cartesian product
            // of the two?
            if !value_column.relationship_path().is_empty() {
                return Err(MongoAgentError::NotImplemented("binary comparisons where the right-side of the comparison references a relationship".into()));
            }

            let left_operand = ColumnRef::from_comparison_target(target_column);
            let right_operand = ColumnRef::from_comparison_target(value_column);
            AggregationExpression(
                operator
                    .mongodb_aggregation_expression(
                        left_operand.into_aggregate_expression(),
                        right_operand.into_aggregate_expression(),
                    )
                    .into(),
            )
        }
        ComparisonValue::Scalar { value, value_type } => {
            let comparison_value = bson_from_scalar_value(value, value_type)?;

            // Special case for array-to-scalar comparisons - this is required because implicit
            // existential quantification over arrays for scalar comparisons does not work in
            // aggregation expressions.
            let expression_doc = if target_column.get_field_type().is_array()
                && !value_type.is_array()
            {
                doc! {
                    "$reduce": {
                        "input": column_expression(target_column),
                        "initialValue": false,
                        "in": operator.mongodb_aggregation_expression("$$this", comparison_value)
                    },
                }
            } else {
                operator.mongodb_aggregation_expression(
                    column_expression(target_column),
                    comparison_value,
                )
            };
            AggregationExpression(expression_doc.into())
        }
        ComparisonValue::Variable {
            name,
            variable_type,
        } => {
            let comparison_value = variable_to_mongo_expression(name, variable_type);
            let expression_doc =
                // Special case for array-to-scalar comparisons - this is required because implicit
                // existential quantification over arrays for scalar comparisons does not work in
                // aggregation expressions.
                if target_column.get_field_type().is_array() && !variable_type.is_array() {
                    doc! {
                        "$reduce": {
                            "input": column_expression(target_column),
                            "initialValue": false,
                            "in": operator.mongodb_aggregation_expression("$$this", comparison_value.into_aggregate_expression())
                        },
                    }
                } else {
                    operator.mongodb_aggregation_expression(
                        column_expression(target_column),
                        comparison_value.into_aggregate_expression()
                    )
                };
            AggregationExpression(expression_doc.into())
        }
    };

    let implicit_exists_over_relationship =
        traverse_relationship_path(target_column.relationship_path(), aggregation_expression);

    Ok(implicit_exists_over_relationship)
}

fn make_unary_comparison_selector(
    target_column: &ndc_query_plan::ComparisonTarget<crate::mongo_query_plan::MongoConfiguration>,
    operator: UnaryComparisonOperator,
) -> std::result::Result<AggregationExpression, crate::interface_types::MongoAgentError> {
    let aggregation_expression = match operator {
        UnaryComparisonOperator::IsNull => AggregationExpression(
            doc! {
                "$eq": [column_expression(target_column), null]
            }
            .into(),
        ),
    };

    let implicit_exists_over_relationship =
        traverse_relationship_path(target_column.relationship_path(), aggregation_expression);

    Ok(implicit_exists_over_relationship)
}

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}

fn traverse_relationship_path(
    relationship_path: &[ndc_models::RelationshipName],
    AggregationExpression(mut expression): AggregationExpression,
) -> AggregationExpression {
    for path_element in relationship_path.iter().rev() {
        let path_element_ref = ColumnRef::from_relationship(path_element);
        expression = doc! {
            "$anyElementTrue": {
                "$map": {
                    "input": path_element_ref.into_aggregate_expression(),
                    "as": "CURRENT", // implicitly changes the document root in `exp` to be the array element
                    "in": expression,
                }
            }
        }
        .into()
    }
    AggregationExpression(expression)
}

fn variable_to_mongo_expression(
    variable: &ndc_models::VariableName,
    value_type: &Type,
) -> ColumnRef<'static> {
    let mongodb_var_name = query_variable_name(variable, value_type);
    ColumnRef::variable(mongodb_var_name)
}
