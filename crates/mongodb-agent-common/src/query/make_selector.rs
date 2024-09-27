use std::iter::once;

use anyhow::anyhow;
use mongodb::bson::{self, doc, Document};
use ndc_models::UnaryComparisonOperator;

use crate::{
    comparison_function::ComparisonFunction,
    interface_types::MongoAgentError,
    mongo_query_plan::{ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type},
    query::column_ref::{column_expression, ColumnRef},
};

use super::{query_variable_name::query_variable_name, serialization::json_to_bson};

pub type Result<T> = std::result::Result<T, MongoAgentError>;

/// Convert a JSON Value into BSON using the provided type information.
/// For example, parses values of type "Date" into BSON DateTime.
fn bson_from_scalar_value(value: &serde_json::Value, value_type: &Type) -> Result<bson::Bson> {
    json_to_bson(value_type, value.clone()).map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))
}

/// Creates a "query document" that filters documents according to the given expression. Query
/// documents are used as arguments for the `$match` aggregation stage, and for the db.find()
/// command.
///
/// Query documents are distinct from "aggregation expressions". The latter are more general.
///
/// TODO: NDC-436 To handle complex expressions with sub-expressions that require a switch to an
/// aggregation expression context we need to turn this into multiple functions to handle context
/// switching. Something like this:
///
///   struct QueryDocument(bson::Document);
///   struct AggregationExpression(bson::Document);
///
///   enum ExpressionPlan {
///     QueryDocument(QueryDocument),
///     AggregationExpression(AggregationExpression),
///   }
///
///   fn make_query_document(expr: &Expression) -> QueryDocument;
///   fn make_aggregate_expression(expr: &Expression) -> AggregationExpression;
///   fn make_expression_plan(exr: &Expression) -> ExpressionPlan;
///
/// The idea is to change `make_selector` to `make_query_document`, and instead of making recursive
/// calls to itself `make_query_document` would make calls to `make_expression_plan` (which would
/// call itself recursively). If any part of the expression plan evaluates to
/// `ExpressionPlan::AggregationExpression(_)` then the entire plan needs to be an aggregation
/// expression, wrapped with the `$expr` query document operator at the top level. So recursion
/// needs to be depth-first.
pub fn make_selector(expr: &Expression) -> Result<Document> {
    match expr {
        Expression::And { expressions } => {
            let sub_exps: Vec<Document> = expressions
                .clone()
                .iter()
                .map(make_selector)
                .collect::<Result<_>>()?;
            Ok(doc! {"$and": sub_exps})
        }
        Expression::Or { expressions } => {
            let sub_exps: Vec<Document> = expressions
                .clone()
                .iter()
                .map(make_selector)
                .collect::<Result<_>>()?;
            Ok(doc! {"$or": sub_exps})
        }
        Expression::Not { expression } => Ok(doc! { "$nor": [make_selector(expression)?]}),
        Expression::Exists {
            in_collection,
            predicate,
        } => Ok(match in_collection {
            ExistsInCollection::Related { relationship } => match predicate {
                Some(predicate) => doc! {
                    relationship.to_string(): { "$elemMatch": make_selector(predicate)? }
                },
                None => doc! { format!("{relationship}.0"): { "$exists": true } },
            },
            // TODO: NDC-434 If a `predicate` is not `None` it should be applied to the unrelated
            // collection
            ExistsInCollection::Unrelated {
                unrelated_collection,
            } => doc! {
                "$expr": {
                    "$ne": [format!("$$ROOT.{unrelated_collection}.0"), null]
                }
            },
            ExistsInCollection::NestedCollection {
                column_name,
                field_path,
                ..
            } => {
                let column_ref =
                    ColumnRef::from_field_path(field_path.iter().chain(once(column_name)));
                match (column_ref, predicate) {
                    (ColumnRef::MatchKey(key), Some(predicate)) => doc! {
                        key: {
                            "$elemMatch": make_selector(predicate)?
                        }
                    },
                    (ColumnRef::MatchKey(key), None) => doc! {
                        key: {
                            "$exists": true,
                            "$not": { "$size": 0 },
                        }
                    },
                    (
                        column_expr @ (ColumnRef::ExpressionStringShorthand(_)
                        | ColumnRef::Expression(_)),
                        Some(predicate),
                    ) => {
                        // TODO: NDC-436 We need to be able to create a plan for `predicate` that
                        // evaluates with the variable `$$this` as document root since that
                        // references each array element. With reference to the plan in the
                        // TODO comment above, this scoped predicate plan needs to be created
                        // with `make_aggregate_expression` since we are in an aggregate
                        // expression context at this point.
                        let predicate_scoped_to_nested_document: Document =
                                Err(MongoAgentError::NotImplemented(format!("currently evaluating the predicate, {predicate:?}, in a nested collection context is not implemented").into()))?;
                        doc! {
                            "$expr": {
                               "$anyElementTrue": {
                                    "$map": {
                                        "input": column_expr.into_aggregate_expression(),
                                        "in": predicate_scoped_to_nested_document,
                                    }
                                }
                            }
                        }
                    }
                    (
                        column_expr @ (ColumnRef::ExpressionStringShorthand(_)
                        | ColumnRef::Expression(_)),
                        None,
                    ) => {
                        doc! {
                            "$expr": {
                                "$gt": [{ "$size": column_expr.into_aggregate_expression() }, 0]
                            }
                        }
                    }
                }
            }
        }),
        Expression::BinaryComparisonOperator {
            column,
            operator,
            value,
        } => make_binary_comparison_selector(column, operator, value),
        Expression::UnaryComparisonOperator { column, operator } => match operator {
            UnaryComparisonOperator::IsNull => {
                let match_doc = match ColumnRef::from_comparison_target(column) {
                    ColumnRef::MatchKey(key) => doc! {
                        key: { "$eq": null }
                    },
                    expr => {
                        // Special case for array-to-scalar comparisons - this is required because implicit
                        // existential quantification over arrays for scalar comparisons does not work in
                        // aggregation expressions.
                        if column.get_field_type().is_array() {
                            doc! {
                                "$expr": {
                                    "$reduce": {
                                        "input": expr.into_aggregate_expression(),
                                        "initialValue": false,
                                        "in": { "$eq": ["$$this", null] }
                                    },
                                },
                            }
                        } else {
                            doc! {
                                "$expr": {
                                    "$eq": [expr.into_aggregate_expression(), null]
                                }
                            }
                        }
                    }
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
                    "binary comparisons between two fields where either field is in a related collection".into(),
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

/// For simple cases the target of an expression is a field reference. But if the target is
/// a column of a related collection then we're implicitly making an array comparison (because
/// related documents always come as an array, even for object relationships), so we have to wrap
/// the starting expression with an `$elemMatch` for each relationship that is traversed to reach
/// the target column.
fn traverse_relationship_path(
    path: &[ndc_models::RelationshipName],
    mut expression: Document,
) -> Document {
    for path_element in path.iter().rev() {
        expression = doc! {
            path_element.to_string(): {
                "$elemMatch": expression
            }
        }
    }
    expression
}

fn variable_to_mongo_expression(
    variable: &ndc_models::VariableName,
    value_type: &Type,
) -> bson::Bson {
    let mongodb_var_name = query_variable_name(variable, value_type);
    format!("$${mongodb_var_name}").into()
}

#[cfg(test)]
mod tests {
    use configuration::MongoScalarType;
    use mongodb::bson::{self, bson, doc};
    use mongodb_support::BsonScalarType;
    use ndc_models::UnaryComparisonOperator;
    use ndc_query_plan::{plan_for_query_request, Scope};
    use ndc_test_helpers::{
        binop, column_value, path_element, query, query_request, relation_field, root, target,
        value,
    };
    use pretty_assertions::assert_eq;

    use crate::{
        comparison_function::ComparisonFunction,
        mongo_query_plan::{
            ComparisonTarget, ComparisonValue, ExistsInCollection, Expression, Type,
        },
        query::pipeline_for_query_request,
        test_helpers::{chinook_config, chinook_relationships},
    };

    use super::make_selector;

    #[test]
    fn compares_fields_of_related_documents_using_elem_match_in_binary_comparison(
    ) -> anyhow::Result<()> {
        let selector = make_selector(&Expression::BinaryComparisonOperator {
            column: ComparisonTarget::Column {
                name: "Name".into(),
                field_path: None,
                field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                path: vec!["Albums".into(), "Tracks".into()],
            },
            operator: ComparisonFunction::Equal,
            value: ComparisonValue::Scalar {
                value: "Helter Skelter".into(),
                value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            },
        })?;

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
        let selector = make_selector(&Expression::UnaryComparisonOperator {
            column: ComparisonTarget::Column {
                name: "Name".into(),
                field_path: None,
                field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                path: vec!["Albums".into(), "Tracks".into()],
            },
            operator: UnaryComparisonOperator::IsNull,
        })?;

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
        let selector = make_selector(&Expression::BinaryComparisonOperator {
            column: ComparisonTarget::Column {
                name: "Name".into(),
                field_path: None,
                field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                path: Default::default(),
            },
            operator: ComparisonFunction::Equal,
            value: ComparisonValue::Column {
                column: ComparisonTarget::Column {
                    name: "Title".into(),
                    field_path: None,
                    field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                    path: Default::default(),
                },
            },
        })?;

        let expected = doc! {
            "$expr": {
                "$eq": ["$Name", "$Title"]
            }
        };

        assert_eq!(selector, expected);
        Ok(())
    }

    #[test]
    fn compares_root_collection_column_to_scalar() -> anyhow::Result<()> {
        let selector = make_selector(&Expression::BinaryComparisonOperator {
            column: ComparisonTarget::ColumnInScope {
                name: "Name".into(),
                field_path: None,
                field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                scope: Scope::Named("scope_0".to_string()),
            },
            operator: ComparisonFunction::Equal,
            value: ComparisonValue::Scalar {
                value: "Lady Gaga".into(),
                value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
            },
        })?;

        let expected = doc! {
            "$expr": {
                "$eq": ["$$scope_0.Name", "Lady Gaga"]
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
                        path_element("Tracks".into()).predicate(
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

        let expected_pipeline = bson!([
            {
                "$lookup": {
                    "from": "Album",
                    "localField": "ArtistId",
                    "foreignField": "ArtistId",
                    "as": "Albums",
                    "let": {
                        "scope_root": "$$ROOT",
                    },
                    "pipeline": [
                        {
                            "$lookup": {
                                "from": "Track",
                                "localField": "AlbumId",
                                "foreignField": "AlbumId",
                                "as": "Tracks",
                                "let": {
                                    "scope_0": "$$ROOT",
                                },
                                "pipeline": [
                                    {
                                        "$match": {
                                            "$expr": { "$eq": ["$Name", "$$scope_0.Title"] },
                                        },
                                    },
                                    {
                                        "$replaceWith": {
                                            "Milliseconds": { "$ifNull": ["$Milliseconds", null] }
                                        }
                                    },
                                ]
                            }
                        },
                        {
                            "$match": {
                                "Tracks": {
                                    "$elemMatch": {
                                        "Milliseconds": { "$gt": 30_000 }
                                    }
                                }
                            }
                        },
                        {
                            "$replaceWith": {
                                "Tracks": { "$getField": { "$literal": "Tracks" } }
                            }
                        },
                    ],
                },
            },
            {
                "$replaceWith": {
                    "Albums": {
                        "rows": []
                    }
                }
            },
        ]);

        assert_eq!(bson::to_bson(&pipeline).unwrap(), expected_pipeline);
        Ok(())
    }

    #[test]
    fn compares_value_to_elements_of_array_field() -> anyhow::Result<()> {
        let selector = make_selector(&Expression::Exists {
            in_collection: ExistsInCollection::NestedCollection {
                column_name: "staff".into(),
                arguments: Default::default(),
                field_path: Default::default(),
            },
            predicate: Some(Box::new(Expression::BinaryComparisonOperator {
                column: ComparisonTarget::Column {
                    name: "last_name".into(),
                    field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                    field_path: Default::default(),
                    path: Default::default(),
                },
                operator: ComparisonFunction::Equal,
                value: ComparisonValue::Scalar {
                    value: "Hughes".into(),
                    value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                },
            })),
        })?;

        let expected = doc! {
            "staff": {
                "$elemMatch": {
                    "last_name": { "$eq": "Hughes" }
                }
            }
        };

        assert_eq!(selector, expected);
        Ok(())
    }

    #[test]
    fn compares_value_to_elements_of_array_field_of_nested_object() -> anyhow::Result<()> {
        let selector = make_selector(&Expression::Exists {
            in_collection: ExistsInCollection::NestedCollection {
                column_name: "staff".into(),
                arguments: Default::default(),
                field_path: vec!["site_info".into()],
            },
            predicate: Some(Box::new(Expression::BinaryComparisonOperator {
                column: ComparisonTarget::Column {
                    name: "last_name".into(),
                    field_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                    field_path: Default::default(),
                    path: Default::default(),
                },
                operator: ComparisonFunction::Equal,
                value: ComparisonValue::Scalar {
                    value: "Hughes".into(),
                    value_type: Type::Scalar(MongoScalarType::Bson(BsonScalarType::String)),
                },
            })),
        })?;

        let expected = doc! {
            "site_info.staff": {
                "$elemMatch": {
                    "last_name": { "$eq": "Hughes" }
                }
            }
        };

        assert_eq!(selector, expected);
        Ok(())
    }
}
