use std::collections::BTreeMap;

use anyhow::anyhow;
use dc_api_types::{
    ArrayComparisonValue, BinaryArrayComparisonOperator, ComparisonValue, ExistsInTable,
    Expression, UnaryComparisonOperator,
};
use mongodb::bson::{self, doc, Document};
use mongodb_support::BsonScalarType;

use crate::{
    comparison_function::ComparisonFunction, interface_types::MongoAgentError,
    query::column_ref::column_ref, query::serialization::json_to_bson_scalar,
};

use BinaryArrayComparisonOperator as ArrOp;

/// Convert a JSON Value into BSON using the provided type information.
/// Parses values of type "date" into BSON DateTime.
fn bson_from_scalar_value(
    value: &serde_json::Value,
    value_type: &str,
) -> Result<bson::Bson, MongoAgentError> {
    // TODO: fail on unrecognized types
    let bson_type = BsonScalarType::from_bson_name(value_type).ok();
    match bson_type {
        Some(t) => {
            json_to_bson_scalar(t, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
        }
        None => bson::to_bson(value).map_err(|e| MongoAgentError::BadQuery(anyhow!(e))),
    }
}

pub fn make_selector(
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    expr: &Expression,
) -> Result<Document, MongoAgentError> {
    make_selector_helper(None, variables, expr)
}

fn make_selector_helper(
    in_table: Option<&str>,
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    expr: &Expression,
) -> Result<Document, MongoAgentError> {
    match expr {
        Expression::And { expressions } => {
            let sub_exps: Vec<Document> = expressions
                .clone()
                .iter()
                .map(|e| make_selector_helper(in_table, variables, e))
                .collect::<Result<_, MongoAgentError>>()?;
            Ok(doc! {"$and": sub_exps})
        }
        Expression::Or { expressions } => {
            let sub_exps: Vec<Document> = expressions
                .clone()
                .iter()
                .map(|e| make_selector_helper(in_table, variables, e))
                .collect::<Result<_, MongoAgentError>>()?;
            Ok(doc! {"$or": sub_exps})
        }
        Expression::Not { expression } => {
            Ok(doc! { "$nor": [make_selector_helper(in_table, variables, expression)?]})
        }
        Expression::Exists { in_table, r#where } => match in_table {
            ExistsInTable::RelatedTable { relationship } => {
                make_selector_helper(Some(relationship), variables, r#where)
            }
            ExistsInTable::UnrelatedTable { .. } => Err(MongoAgentError::NotImplemented(
                "filtering on an unrelated table",
            )),
        },
        Expression::ApplyBinaryComparison {
            column,
            operator,
            value,
        } => {
            let mongo_op = ComparisonFunction::try_from(operator)?;
            let col = column_ref(column, in_table)?;
            let comparison_value = match value {
                ComparisonValue::AnotherColumnComparison { .. } => Err(
                    MongoAgentError::NotImplemented("comparisons between columns"),
                ),
                ComparisonValue::ScalarValueComparison { value, value_type } => {
                    bson_from_scalar_value(value, value_type)
                }
                ComparisonValue::Variable { name } => {
                    variable_to_mongo_expression(variables, name, &column.column_type)
                        .map(Into::into)
                }
            }?;
            Ok(mongo_op.mongodb_expression(col, comparison_value))
        }
        Expression::ApplyBinaryArrayComparison {
            column,
            operator,
            value_type,
            values,
        } => {
            let mongo_op = match operator {
                ArrOp::In => "$in",
                ArrOp::CustomBinaryComparisonOperator(op) => op,
            };
            let values: Vec<bson::Bson> = values
                .iter()
                .map(|value| match value {
                    ArrayComparisonValue::Scalar(value) => {
                        bson_from_scalar_value(value, value_type)
                    }
                    ArrayComparisonValue::Column(_column) => Err(MongoAgentError::NotImplemented(
                        "comparisons between columns",
                    )),
                    ArrayComparisonValue::Variable(name) => {
                        variable_to_mongo_expression(variables, name, value_type)
                    }
                })
                .collect::<Result<_, MongoAgentError>>()?;
            Ok(doc! {
                column_ref(column, in_table)?: {
                    mongo_op: values
                }
            })
        }
        Expression::ApplyUnaryComparison { column, operator } => match operator {
            UnaryComparisonOperator::IsNull => {
                // Checks the type of the column - type 10 is the code for null. This differs from
                // `{ "$eq": null }` in that the checking equality with null returns true if the
                // value is null or is absent. Checking for type 10 returns true if the value is
                // null, but false if it is absent.
                Ok(doc! {
                    column_ref(column, in_table)?: { "$type": 10 }
                })
            }
            UnaryComparisonOperator::CustomUnaryComparisonOperator(op) => {
                let col = column_ref(column, in_table)?;
                if op == "$exists" {
                    Ok(doc! { col: { "$exists": true } })
                } else {
                    // TODO: Is `true` the proper value here?
                    Ok(doc! { col: { op: true } })
                }
            }
        },
    }
}

fn variable_to_mongo_expression(
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    variable: &str,
    value_type: &str,
) -> Result<bson::Bson, MongoAgentError> {
    let value = variables
        .and_then(|vars| vars.get(variable))
        .ok_or_else(|| MongoAgentError::VariableNotDefined(variable.to_owned()))?;

    bson_from_scalar_value(value, value_type)
}
