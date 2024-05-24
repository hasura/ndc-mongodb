use std::collections::BTreeMap;

use anyhow::anyhow;
use mongodb::bson::{self, doc, Document};
use ndc_models::UnaryComparisonOperator;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonValue, ExistsInCollection, Expression, Type},
    query::column_ref::column_ref,
};

use super::serialization::json_to_bson;

pub type Result<T> = std::result::Result<T, MongoAgentError>;

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}

pub fn make_selector(
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    expr: &Expression,
) -> Result<Document> {
    match expr {
        Expression::And { expressions } => {
            let sub_exps: Vec<Document> = expressions
                .clone()
                .iter()
                .map(|e| make_selector(variables, e))
                .collect::<Result<_>>()?;
            Ok(doc! {"$and": sub_exps})
        }
        Expression::Or { expressions } => {
            let sub_exps: Vec<Document> = expressions
                .clone()
                .iter()
                .map(|e| make_selector(variables, e))
                .collect::<Result<_>>()?;
            Ok(doc! {"$or": sub_exps})
        }
        Expression::Not { expression } => {
            Ok(doc! { "$nor": [make_selector(variables, expression)?]})
        }
        Expression::Exists {
            in_collection,
            predicate,
        } => Ok(match in_collection {
            ExistsInCollection::Related { relationship } => match predicate {
                Some(predicate) => doc! {
                    format!("${relationship}"): { "$elemMatch": make_selector(variables, predicate)? }
                },
                None => doc! { format!("${relationship}.0"): { "$exists": true } },
            },
            ExistsInCollection::Unrelated {
                unrelated_collection,
            } => doc! { format!("$$ROOT.{unrelated_collection}.0"): { "$exists": true } },
        }),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => {
            let col = column_ref(column)?;
            let comparison_value = match value {
                // TODO: MDB-152 To compare to another column we need to wrap the entire expression in
                // an `$expr` aggregation operator (assuming the expression is not already in
                // an aggregation expression context)
                ComparisonValue::Column { .. } => Err(MongoAgentError::NotImplemented(
                    "comparisons between columns",
                )),
                ComparisonValue::Scalar { value, value_type } => {
                    bson_from_scalar_value(value, value_type)
                }
                ComparisonValue::Variable {
                    name,
                    variable_type,
                } => variable_to_mongo_expression(variables, name, variable_type).map(Into::into),
            }?;
            Ok(operator.mongodb_expression(col.into_owned(), comparison_value))
        }
        Expression::UnaryComparisonOperator { column, operator } => match operator {
            UnaryComparisonOperator::IsNull => {
                // Checks the type of the column - type 10 is the code for null. This differs from
                // `{ "$eq": null }` in that the checking equality with null returns true if the
                // value is null or is absent. Checking for type 10 returns true if the value is
                // null, but false if it is absent.
                Ok(doc! {
                    column_ref(column)?: { "$type": 10 }
                })
            }
        },
    }
}

fn variable_to_mongo_expression(
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    variable: &str,
    value_type: &Type,
) -> Result<bson::Bson> {
    let value = variables
        .and_then(|vars| vars.get(variable))
        .ok_or_else(|| MongoAgentError::VariableNotDefined(variable.to_owned()))?;

    bson_from_scalar_value(value, value_type)
}
