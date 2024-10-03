use std::iter::once;

use mongodb::bson::{doc, Bson};

use crate::{
    comparison_function::ComparisonFunction, mongo_query_plan::{ComparisonTarget, ComparisonValue, ExistsInCollection, Expression}, query::column_ref::{column_expression, ColumnRef}
};

use super::Result;

pub struct AggregationExpression(pub Bson);

pub fn make_aggregation_expression(expr: &Expression) -> Result<AggregationExpression> {
    todo!("make_aggregation_expression")
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
    let selector = match value {
        ComparisonValue::Column {
            column: value_column,
        } => {
            let left_operand = ColumnRef::from_comparison_target(target_column);
            let right_operand = ColumnRef::from_comparison_target(value_column);
            AggregationExpression(operator.mongodb_aggregation_expression(
                left_operand.into_aggregate_expression(),
                right_operand.into_aggregate_expression(),
            ))
        }
        ComparisonValue::Scalar { value, value_type } => {
            let comparison_value = bson_from_scalar_value(value, value_type)?;
            let match_doc = match ColumnRef::from_comparison_target(target_column) {
                ColumnRef::MatchKey(key) => operator.mongodb_match_query(key, comparison_value),
                expr => {
                    // Special case for array-to-scalar comparisons - this is required because implicit
                    // existential quantification over arrays for scalar comparisons does not work in
                    // aggregation expressions.
                    if target_column.get_field_type().is_array() && !value_type.is_array() {
                        doc! {
                            "$expr": {
                                "$reduce": {
                                    "input": expr.into_aggregate_expression(),
                                    "initialValue": false,
                                    "in": operator.mongodb_aggregation_expression("$$this", comparison_value)
                                },
                            },
                        }
                    } else {
                        doc! {
                            "$expr": operator.mongodb_aggregation_expression(expr.into_aggregate_expression(), comparison_value)
                        }
                    }
                }
            };
            traverse_relationship_path(target_column.relationship_path(), match_doc)
        }
        ComparisonValue::Variable {
            name,
            variable_type,
        } => {
            let comparison_value = variable_to_mongo_expression(name, variable_type);
            let match_doc =
                // Special case for array-to-scalar comparisons - this is required because implicit
                // existential quantification over arrays for scalar comparisons does not work in
                // aggregation expressions.
                if target_column.get_field_type().is_array() && !variable_type.is_array() {
                    doc! {
                        "$expr": {
                            "$reduce": {
                                "input": column_expression(target_column),
                                "initialValue": false,
                                "in": operator.mongodb_aggregation_expression("$$this", comparison_value)
                            },
                        },
                    }
                } else {
                    doc! {
                        "$expr": operator.mongodb_aggregation_expression(
                            column_expression(target_column),
                            comparison_value
                        )
                    }
                };
            traverse_relationship_path(target_column.relationship_path(), match_doc)
        }
    };
    Ok(selector)
}

fn bson_from_scalar_value(value: &serde_json::Value, value_type: &ndc_query_plan::Type<configuration::MongoScalarType>) -> _ {
    todo!()
}

fn traverse_relationship_path(relationship_path: &[ndc_models::RelationshipName], match_doc: mongodb::bson::Document) -> AggregationExpression {
    todo!()
}

fn variable_to_mongo_expression(
    variable: &ndc_models::VariableName,
    value_type: &Type,
) -> bson::Bson {
    let mongodb_var_name = query_variable_name(variable, value_type);
    format!("$${mongodb_var_name}").into()
}
