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

impl From<AggregationExpression> for Bson {
    fn from(value: AggregationExpression) -> Self {
        value.into_bson()
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
        Expression::ArrayComparison { column, comparison } => {
            make_array_comparison_selector(column, comparison)
        }
        Expression::UnaryComparisonOperator { column, operator } => {
            Ok(make_unary_comparison_selector(column, *operator))
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
            exists_in_array(column_ref, predicate)?
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
                    "as": "CURRENT", // implicitly changes the document root in `sub_expression` to be the array element
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
    let left_operand = ColumnRef::from_comparison_target(target_column).into_aggregate_expression();
    let right_operand = value_expression(value)?;
    let expr = AggregationExpression(
        operator
            .mongodb_aggregation_expression(left_operand, right_operand)
            .into(),
    );
    Ok(expr)
}

fn make_unary_comparison_selector(
    target_column: &ndc_query_plan::ComparisonTarget<crate::mongo_query_plan::MongoConfiguration>,
    operator: UnaryComparisonOperator,
) -> AggregationExpression {
    match operator {
        UnaryComparisonOperator::IsNull => AggregationExpression(
            doc! {
                "$eq": [column_expression(target_column), null]
            }
            .into(),
        ),
    }
}

fn make_array_comparison_selector(
    column: &ComparisonTarget,
    comparison: &ArrayComparison,
) -> Result<AggregationExpression> {
    let doc = match comparison {
        ArrayComparison::Contains { value } => doc! {
            "$in": [value_expression(value)?, column_expression(column)]
        },
        ArrayComparison::IsEmpty => todo!(),
    };
    Ok(AggregationExpression(doc.into()))
}

fn value_expression(value: &ComparisonValue) -> Result<AggregationExpression> {
    match value {
        ComparisonValue::Column {
            path,
            name,
            field_path,
            scope: _, // We'll need to reference scope for ENG-1153
            ..
        } => {
            // TODO: ENG-1153 Do we want an implicit exists in the value relationship? If both
            // target and value reference relationships do we want an exists in a Cartesian product
            // of the two?
            if !path.is_empty() {
                return Err(MongoAgentError::NotImplemented("binary comparisons where the right-side of the comparison references a relationship".into()));
            }

            let value_ref = ColumnRef::from_column_and_field_path(name, field_path.as_ref());
            Ok(value_ref.into_aggregate_expression())
        }
        ComparisonValue::Scalar { value, value_type } => {
            let comparison_value = bson_from_scalar_value(value, value_type)?;
            Ok(AggregationExpression(comparison_value))
        }
        ComparisonValue::Variable {
            name,
            variable_type,
        } => {
            let comparison_value = variable_to_mongo_expression(name, variable_type);
            Ok(comparison_value.into_aggregate_expression())
        }
    }
}

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}

fn variable_to_mongo_expression(
    variable: &ndc_models::VariableName,
    value_type: &Type,
) -> ColumnRef<'static> {
    let mongodb_var_name = query_variable_name(variable, value_type);
    ColumnRef::variable(mongodb_var_name)
}
