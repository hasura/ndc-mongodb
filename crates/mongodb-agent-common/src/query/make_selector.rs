use std::collections::BTreeMap;

use anyhow::anyhow;
use mongodb::bson::{self, doc, Document};
use ndc_models::UnaryComparisonOperator;

use crate::{
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type},
    query::column_ref::{column_expression, ColumnRef},
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
                    relationship: { "$elemMatch": make_selector(variables, predicate)? }
                },
                None => doc! { format!("{relationship}.0"): { "$exists": true } },
            },
            ExistsInCollection::Unrelated {
                unrelated_collection,
            } => doc! {
                "$expr": {
                    "$ne": [format!("$$ROOT.{unrelated_collection}.0"), null]
                }
            },
        }),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => make_binary_comparison_selector(variables, column, operator, value),
        Expression::UnaryComparisonOperator { column, operator } => match operator {
            UnaryComparisonOperator::IsNull => {
                let match_doc = match ColumnRef::from_comparison_target(column) {
                    ColumnRef::MatchKey(key) => doc! {
                        key: { "$eq": null }
                    },
                    ColumnRef::Expression(expr) => doc! {
                        "$expr": {
                            "$eq": [expr, null]
                        }
                    },
                };
                Ok(traverse_relationship_path(
                    column.relationship_path(),
                    match_doc,
                ))
            }
        },
    }
}

fn make_binary_comparison_selector(
    variables: Option<&BTreeMap<String, serde_json::Value>>,
    target_column: &ComparisonTarget,
    operator: &ComparisonFunction,
    value: &ComparisonValue,
) -> Result<Document> {
    let selector = match value {
        ComparisonValue::Column {
            column: value_column,
        } => {
            if !target_column.relationship_path().is_empty()
                || !value_column.relationship_path().is_empty()
            {
                return Err(MongoAgentError::NotImplemented(
                    "binary comparisons between two fields where either field is in a related collection",
                ));
            }
            doc! {
                "$expr": operator.mongodb_aggregation_expression(
                    column_expression(target_column),
                    column_expression(value_column)
                )
            }
        }
        ComparisonValue::Scalar { value, value_type } => {
            let comparison_value = bson_from_scalar_value(value, value_type)?;
            let match_doc = match ColumnRef::from_comparison_target(target_column) {
                ColumnRef::MatchKey(key) => operator.mongodb_match_query(key, comparison_value),
                ColumnRef::Expression(expr) => {
                    operator.mongodb_aggregation_expression(expr, comparison_value)
                }
            };
            traverse_relationship_path(target_column.relationship_path(), match_doc)
        }
        ComparisonValue::Variable {
            name,
            variable_type,
        } => {
            let comparison_value =
                variable_to_mongo_expression(variables, name, variable_type).map(Into::into)?;
            let match_doc = match ColumnRef::from_comparison_target(target_column) {
                ColumnRef::MatchKey(key) => operator.mongodb_match_query(key, comparison_value),
                ColumnRef::Expression(expr) => {
                    operator.mongodb_aggregation_expression(expr, comparison_value)
                }
            };
            traverse_relationship_path(target_column.relationship_path(), match_doc)
        }
    };
    Ok(selector)
}

/// For simple cases the target of an expression is a field reference. But if the target is
/// a column of a related collection then we're implicitly making an array comparison (because
/// related documents always come as an array, even for object relationships), so we have to wrap
/// the starting expression with an `$elemMatch` for each relationship that is traversed to reach
/// the target column.
fn traverse_relationship_path(path: &[String], mut expression: Document) -> Document {
    for path_element in path.iter().rev() {
        expression = doc! {
            path_element: {
                "$elemMatch": expression
            }
        }
    }
    expression
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

#[cfg(test)]
mod tests {
    use configuration::MongoScalarType;
    use mongodb::bson::doc;
    use mongodb_support::BsonScalarType;
    use ndc_models::UnaryComparisonOperator;
    use pretty_assertions::assert_eq;

    use crate::{
        comparison_function::ComparisonFunction,
        mongo_query_plan::{ComparisonTarget, ComparisonValue, Expression, Type},
    };

    use super::make_selector;

    #[test]
    fn compares_fields_of_related_documents_using_elem_match_in_binary_comparison(
    ) -> anyhow::Result<()> {
        let selector = make_selector(
            None,
            &Expression::BinaryComparisonOperator {
                column: ComparisonTarget::Column {
                    name: "Name".to_owned(),
                    field_path: None,
                    column_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                    path: vec!["Albums".into(), "Tracks".into()],
                },
                operator: ComparisonFunction::Equal,
                value: ComparisonValue::Scalar {
                    value: "Helter Skelter".into(),
                    value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                },
            },
        )?;

        let expected = doc! {
            "Albums": {
                "$elemMatch": {
                    "Tracks": {
                        "$elemMatch": {
                            "Name": { "$eq": "Helter Skelter" }
                        }
                    }
                }
            }
        };

        assert_eq!(selector, expected);
        Ok(())
    }

    #[test]
    fn compares_fields_of_related_documents_using_elem_match_in_unary_comparison(
    ) -> anyhow::Result<()> {
        let selector = make_selector(
            None,
            &Expression::UnaryComparisonOperator {
                column: ComparisonTarget::Column {
                    name: "Name".to_owned(),
                    field_path: None,
                    column_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                    path: vec!["Albums".into(), "Tracks".into()],
                },
                operator: UnaryComparisonOperator::IsNull,
            },
        )?;

        let expected = doc! {
            "Albums": {
                "$elemMatch": {
                    "Tracks": {
                        "$elemMatch": {
                            "Name": { "$eq": null }
                        }
                    }
                }
            }
        };

        assert_eq!(selector, expected);
        Ok(())
    }

    #[test]
    fn compares_two_columns() -> anyhow::Result<()> {
        let selector = make_selector(
            None,
            &Expression::BinaryComparisonOperator {
                column: ComparisonTarget::Column {
                    name: "Name".to_owned(),
                    field_path: None,
                    column_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                    path: Default::default(),
                },
                operator: ComparisonFunction::Equal,
                value: ComparisonValue::Column {
                    column: ComparisonTarget::Column {
                        name: "Title".to_owned(),
                        field_path: None,
                        column_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                        path: Default::default(),
                    },
                },
            },
        )?;

        let expected = doc! {
            "$expr": {
                "$eq": ["$Name", "$Title"]
            }
        };

        assert_eq!(selector, expected);
        Ok(())
    }
}
