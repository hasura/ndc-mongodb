mod make_aggregation_expression;
mod make_expression_plan;
mod make_query_document;

use mongodb::bson::{doc, Document};

use crate::{interface_types::MongoAgentError, mongo_query_plan::Expression};

pub use self::{
    make_aggregation_expression::AggregationExpression,
    make_expression_plan::{make_expression_plan, ExpressionPlan},
    make_query_document::QueryDocument,
};

pub type Result<T> = std::result::Result<T, MongoAgentError>;

/// Creates a "query document" that filters documents according to the given expression. Query
/// documents are used as arguments for the `$match` aggregation stage, and for the db.find()
/// command.
///
/// Query documents are distinct from "aggregation expressions". The latter are more general.
pub fn make_selector(expr: &Expression) -> Result<Document> {
    let selector = match make_expression_plan(expr)? {
        ExpressionPlan::QueryDocument(QueryDocument(doc)) => doc,
        ExpressionPlan::AggregationExpression(AggregationExpression(e)) => doc! {
            "$expr": e,
        },
    };
    Ok(selector)
}

#[cfg(test)]
mod tests {
    use configuration::MongoScalarType;
    use mongodb::bson::{self, bson, doc};
    use mongodb_support::{aggregate::AggregateCommand, BsonScalarType};
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
        query::command_for_query_request,
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
    fn root_column_reference_references_column_of_nearest_query() -> anyhow::Result<()> {
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
        let AggregateCommand {
            collection: _,
            pipeline,
            let_vars,
        } = command_for_query_request(&config, &plan)?;

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
        assert_eq!(let_vars, None);
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
