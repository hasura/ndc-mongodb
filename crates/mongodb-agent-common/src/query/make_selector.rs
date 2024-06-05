use std::{borrow::Cow, collections::BTreeMap, iter::once};

use anyhow::anyhow;
use itertools::Either;
use mongodb::bson::{self, doc, Document};
use ndc_models::UnaryComparisonOperator;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type},
    mongodb::sanitize::safe_name,
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
            } => doc! { format!("$$ROOT.{unrelated_collection}.0"): { "$exists": true } },
        }),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => {
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
            Ok(traverse_relationship_path(
                column.relationship_path(),
                operator.mongodb_expression(column_ref(column)?, comparison_value),
            ))
        }
        Expression::UnaryComparisonOperator { column, operator } => match operator {
            UnaryComparisonOperator::IsNull => Ok(traverse_relationship_path(
                column.relationship_path(),
                doc! { column_ref(column)?: { "$eq": null } },
            )),
        },
    }
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

/// Given a column target returns a MongoDB expression that resolves to the value of the
/// corresponding field, either in the target collection of a query request, or in the related
/// collection. Resolves nested fields, but does not traverse relationships.
fn column_ref(column: &ComparisonTarget) -> Result<Cow<'_, str>> {
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

#[cfg(test)]
mod tests {
    use configuration::MongoScalarType;
    use mongodb::bson::{self, bson, doc};
    use mongodb_support::BsonScalarType;
    use ndc_models::UnaryComparisonOperator;
    use ndc_query_plan::plan_for_query_request;
    use ndc_test_helpers::{
        binop, column_value, path_element, query, query_request, relation_field, root, target,
        value,
    };
    use pretty_assertions::assert_eq;

    use crate::{
        comparison_function::ComparisonFunction,
        mongo_query_plan::{ComparisonTarget, ComparisonValue, Expression, Type},
        query::pipeline_for_query_request,
        test_helpers::{chinook_config, chinook_relationships},
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
    fn root_column_reference_refereces_column_of_nearest_query() -> anyhow::Result<()> {
        let request = query_request()
            .collection("Artist")
            .query(
                query().fields([relation_field!("Albums" => "Albums", query().predicate(
                binop(
                    "_gt",
                    target!("Milliseconds", relations: [
                        path_element("Tracks").predicate(
                            binop("_eq", target!("Name"), column_value!(root("Title")))
                        ),
                    ]),
                    value!(30_000),
                )
                ))]),
            )
            .relationships(chinook_relationships())
            .into();

        let config = chinook_config();
        let plan = plan_for_query_request(&config, request)?;
        let pipeline = pipeline_for_query_request(&config, &plan)?;

        let expected_pipeline = bson!([]);

        assert_eq!(bson::to_bson(&pipeline).unwrap(), expected_pipeline);
        Ok(())
    }
}
