use std::collections::HashMap;

use dc_api_types::comparison_column::ColumnSelector;
use dc_api_types::{
    BinaryComparisonOperator, ComparisonColumn, ComparisonValue, Expression, QueryRequest,
    ScalarValue, VariableSet,
};
use mongodb::bson::{doc, Bson};

use super::pipeline::{pipeline_for_non_foreach, ResponseShape};
use crate::mongodb::Selection;
use crate::{
    interface_types::MongoAgentError,
    mongodb::{Pipeline, Stage},
};

const FACET_FIELD: &str = "__FACET__";

/// If running a native v2 query we will get `Expression` values. If the query is translated from
/// v3 we will get variable sets instead.
#[derive(Clone, Debug)]
pub enum ForeachVariant {
    Predicate(Expression),
    VariableSet(VariableSet),
}

/// If the query request represents a "foreach" query then we will need to run multiple variations
/// of the query represented by added predicates and variable sets. This function returns a vec in
/// that case. If the returned map is `None` then the request is not a "foreach" query.
pub fn foreach_variants(query_request: &QueryRequest) -> Option<Vec<ForeachVariant>> {
    if let Some(Some(foreach)) = &query_request.foreach {
        let expressions = foreach
            .iter()
            .map(make_expression)
            .map(ForeachVariant::Predicate)
            .collect();
        Some(expressions)
    } else if let Some(variables) = &query_request.variables {
        let variable_sets = variables
            .iter()
            .cloned()
            .map(ForeachVariant::VariableSet)
            .collect();
        Some(variable_sets)
    } else {
        None
    }
}

/// Produces a complete MongoDB pipeline for a foreach query.
///
/// For symmetry with [`super::execute_query_request::pipeline_for_query`] and
/// [`pipeline_for_non_foreach`] this function returns a pipeline paired with a value that
/// indicates whether the response requires post-processing in the agent.
pub fn pipeline_for_foreach(
    foreach: Vec<ForeachVariant>,
    query_request: &QueryRequest,
) -> Result<(Pipeline, ResponseShape), MongoAgentError> {
    let pipelines_with_response_shapes: Vec<(String, (Pipeline, ResponseShape))> = foreach
        .into_iter()
        .enumerate()
        .map(|(index, foreach_variant)| {
            let (predicate, variables) = match foreach_variant {
                ForeachVariant::Predicate(expression) => (Some(expression), None),
                ForeachVariant::VariableSet(variables) => (None, Some(variables)),
            };
            let mut q = query_request.clone();

            if let Some(predicate) = predicate {
                q.query.r#where = match q.query.r#where {
                    Some(e_old) => e_old.and(predicate),
                    None => predicate,
                }
                .into();
            }

            let pipeline_with_response_shape = pipeline_for_non_foreach(variables.as_ref(), &q)?;
            Ok((facet_name(index), pipeline_with_response_shape))
        })
        .collect::<Result<_, MongoAgentError>>()?;

    let selection = Selection(doc! {
        "rows": pipelines_with_response_shapes.iter().map(|(key, (_, response_shape))| doc! {
            "query": match response_shape {
                ResponseShape::RowStream => doc! { "rows": format!("${key}") }.into(),
                ResponseShape::SingleObject => Bson::String(format!("${key}")),
            }
        }).collect::<Vec<_>>()
    });

    let queries = pipelines_with_response_shapes
        .into_iter()
        .map(|(key, (pipeline, _))| (key, pipeline))
        .collect();

    Ok((
        Pipeline {
            stages: vec![Stage::Facet(queries), Stage::ReplaceWith(selection)],
        },
        ResponseShape::SingleObject,
    ))
}

/// Fold a 'foreach' HashMap into an Expression.
fn make_expression(column_values: &HashMap<String, ScalarValue>) -> Expression {
    let sub_exps: Vec<Expression> = column_values
        .clone()
        .into_iter()
        .map(
            |(column_name, scalar_value)| Expression::ApplyBinaryComparison {
                column: ComparisonColumn {
                    column_type: scalar_value.value_type.clone(),
                    name: ColumnSelector::new(column_name),
                    path: None,
                },
                operator: BinaryComparisonOperator::Equal,
                value: ComparisonValue::ScalarValueComparison {
                    value: scalar_value.value,
                    value_type: scalar_value.value_type,
                },
            },
        )
        .collect();

    Expression::And {
        expressions: sub_exps,
    }
}

fn facet_name(index: usize) -> String {
    format!("{FACET_FIELD}_{index}")
}

#[cfg(test)]
mod tests {
    use dc_api_types::{
        BinaryComparisonOperator, ComparisonColumn, Field, Query, QueryRequest, QueryResponse,
    };
    use mongodb::{
        bson::{doc, from_document},
        options::AggregateOptions,
    };
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json, to_value};

    use crate::{
        mongodb::{test_helpers::mock_stream, MockCollectionTrait},
        query::execute_query_request::execute_query_request,
    };

    #[tokio::test]
    async fn executes_foreach_with_fields() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                  "albumId": {
                    "type": "column",
                    "column": "albumId",
                    "column_type": "number"
                  },
                  "title": {
                    "type": "column",
                    "column": "title",
                    "column_type": "string"
                  }
                }
            },
            "target": {"name": ["tracks"], "type": "table"},
            "relationships": [],
            "foreach": [
                { "artistId": {"value": 1, "value_type": "int"} },
                { "artistId": {"value": 2, "value_type": "int"} }
            ]
        }))?;

        let expected_pipeline = json!([
            {
                "$facet": {
                    "__FACET___0": [
                        { "$match": { "$and": [{ "artistId": {"$eq":1 }}]}},
                        { "$replaceWith": {
                            "albumId": { "$ifNull": ["$albumId", null] },
                            "title": { "$ifNull": ["$title", null] }
                        } },
                    ],
                    "__FACET___1": [
                        { "$match": { "$and": [{ "artistId": {"$eq":2}}]}},
                        { "$replaceWith": {
                            "albumId": { "$ifNull": ["$albumId", null] },
                            "title": { "$ifNull": ["$title", null] }
                        } },
                    ]
                },
            },
            {
                "$replaceWith": {
                    "rows": [
                        { "query": { "rows": "$__FACET___0" } },
                        { "query": { "rows": "$__FACET___1" } },
                    ]
                },
            }
        ]);

        let expected_response: QueryResponse = from_value(json! ({
            "rows": [
                {
                    "query": {
                        "rows": [
                            { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                            { "albumId": 4, "title": "Let There Be Rock" }
                        ]
                    }
                },
                {
                    "query": {
                        "rows": [
                            { "albumId": 2, "title": "Balls to the Wall" },
                            { "albumId": 3, "title": "Restless and Wild" }
                        ]
                    }
                }
            ]
        }))?;

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_value(pipeline).unwrap());
                Ok(mock_stream(vec![Ok(from_document(doc! {
                    "rows": [
                        {
                            "query": {
                                "rows": [
                                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                                    { "albumId": 4, "title": "Let There Be Rock" }
                                ]
                            }
                        },
                        {
                            "query": {
                                "rows": [
                                    { "albumId": 2, "title": "Balls to the Wall" },
                                    { "albumId": 3, "title": "Restless and Wild" }
                                ]
                            }
                        }
                    ],
                })?)]))
            });

        let result = execute_query_request(&collection, query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_foreach_with_aggregates() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "aggregates": {
                   "count": { "type": "star_count" },
                },
                "fields": {
                  "albumId": {
                    "type": "column",
                    "column": "albumId",
                    "column_type": "number"
                  },
                  "title": {
                    "type": "column",
                    "column": "title",
                    "column_type": "string"
                  }
                }
            },
            "target": {"name": ["tracks"], "type": "table"},
            "relationships": [],
            "foreach": [
                { "artistId": {"value": 1, "value_type": "int"} },
                { "artistId": {"value": 2, "value_type": "int"} }
            ]
        }))?;

        let expected_pipeline = json!([
            {
                "$facet": {
                    "__FACET___0": [
                        { "$match": { "$and": [{ "artistId": {"$eq": 1 }}]}},
                        { "$facet": {
                            "__ROWS__": [{ "$replaceWith": {
                                "albumId": { "$ifNull": ["$albumId", null] },
                                "title": { "$ifNull": ["$title", null] }
                            }}],
                            "count": [{ "$count": "result" }],
                        } },
                        { "$replaceWith": {
                            "aggregates": {
                                "count": { "$getField": {
                                    "field": "result",
                                    "input": { "$first": { "$getField": { "$literal": "count" } } }
                                } },
                            },
                            "rows": "$__ROWS__",
                        } },
                    ],
                    "__FACET___1": [
                        { "$match": { "$and": [{ "artistId": {"$eq": 2 }}]}},
                        { "$facet": {
                            "__ROWS__": [{ "$replaceWith": {
                                "albumId": { "$ifNull": ["$albumId", null] },
                                "title": { "$ifNull": ["$title", null] }
                            }}],
                            "count": [{ "$count": "result" }],
                        } },
                        { "$replaceWith": {
                            "aggregates": {
                                "count": { "$getField": {
                                    "field": "result",
                                    "input": { "$first": { "$getField": { "$literal": "count" } } }
                                } },
                            },
                            "rows": "$__ROWS__",
                        } },
                    ]
                },
            },
            {
                "$replaceWith": {
                    "rows": [
                        { "query": "$__FACET___0" },
                        { "query": "$__FACET___1" },
                    ]
                },
            }
        ]);

        let expected_response: QueryResponse = from_value(json! ({
            "rows": [
                {
                    "query": {
                        "aggregates": {
                            "count": 2,
                        },
                        "rows": [
                            { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                            { "albumId": 4, "title": "Let There Be Rock" }
                        ]
                    }
                },
                {
                    "query": {
                        "aggregates": {
                            "count": 2,
                        },
                        "rows": [
                            { "albumId": 2, "title": "Balls to the Wall" },
                            { "albumId": 3, "title": "Restless and Wild" }
                        ]
                    }
                }
            ]
        }))?;

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_value(pipeline).unwrap());
                Ok(mock_stream(vec![Ok(from_document(doc! {
                    "rows": [
                        {
                            "query": {
                                "aggregates": {
                                    "count": 2,
                                },
                                "rows": [
                                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                                    { "albumId": 4, "title": "Let There Be Rock" }
                                ]
                            }
                        },
                        {
                            "query": {
                                "aggregates": {
                                    "count": 2,
                                },
                                "rows": [
                                    { "albumId": 2, "title": "Balls to the Wall" },
                                    { "albumId": 3, "title": "Restless and Wild" }
                                ]
                            }
                        }
                    ],
                })?)]))
            });

        let result = execute_query_request(&collection, query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_foreach_with_variables() -> Result<(), anyhow::Error> {
        let query_request = QueryRequest {
            foreach: None,
            variables: Some(
                (1..=12)
                    .into_iter()
                    .map(|artist_id| [("artistId".to_owned(), json!(artist_id))].into())
                    .collect(),
            ),
            target: dc_api_types::Target::TTable {
                name: vec!["tracks".to_owned()],
            },
            relationships: Default::default(),
            query: Box::new(Query {
                r#where: Some(dc_api_types::Expression::ApplyBinaryComparison {
                    column: ComparisonColumn::new(
                        "int".to_owned(),
                        dc_api_types::ColumnSelector::Column("artistId".to_owned()),
                    ),
                    operator: BinaryComparisonOperator::Equal,
                    value: dc_api_types::ComparisonValue::Variable {
                        name: "artistId".to_owned(),
                    },
                }),
                fields: Some(Some(
                    [
                        (
                            "albumId".to_owned(),
                            Field::Column {
                                column: "albumId".to_owned(),
                                column_type: "int".to_owned(),
                            },
                        ),
                        (
                            "title".to_owned(),
                            Field::Column {
                                column: "title".to_owned(),
                                column_type: "string".to_owned(),
                            },
                        ),
                    ]
                    .into(),
                )),
                aggregates: None,
                aggregates_limit: None,
                limit: None,
                offset: None,
                order_by: None,
            }),
        };

        fn facet(artist_id: i32) -> serde_json::Value {
            json!([
                { "$match": { "artistId": {"$eq": artist_id } } },
                { "$replaceWith": {
                    "albumId": { "$ifNull": ["$albumId", null] },
                    "title": { "$ifNull": ["$title", null] }
                } },
            ])
        }

        let expected_pipeline = json!([
            {
                "$facet": {
                    "__FACET___0": facet(1),
                    "__FACET___1": facet(2),
                    "__FACET___2": facet(3),
                    "__FACET___3": facet(4),
                    "__FACET___4": facet(5),
                    "__FACET___5": facet(6),
                    "__FACET___6": facet(7),
                    "__FACET___7": facet(8),
                    "__FACET___8": facet(9),
                    "__FACET___9": facet(10),
                    "__FACET___10": facet(11),
                    "__FACET___11": facet(12),
                },
            },
            {
                "$replaceWith": {
                    "rows": [
                        { "query": { "rows": "$__FACET___0" } },
                        { "query": { "rows": "$__FACET___1" } },
                        { "query": { "rows": "$__FACET___2" } },
                        { "query": { "rows": "$__FACET___3" } },
                        { "query": { "rows": "$__FACET___4" } },
                        { "query": { "rows": "$__FACET___5" } },
                        { "query": { "rows": "$__FACET___6" } },
                        { "query": { "rows": "$__FACET___7" } },
                        { "query": { "rows": "$__FACET___8" } },
                        { "query": { "rows": "$__FACET___9" } },
                        { "query": { "rows": "$__FACET___10" } },
                        { "query": { "rows": "$__FACET___11" } },
                    ]
                },
            }
        ]);

        let expected_response: QueryResponse = from_value(json! ({
            "rows": [
                {
                    "query": {
                        "rows": [
                            { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                            { "albumId": 4, "title": "Let There Be Rock" }
                        ]
                    }
                },
                { "query": { "rows": [] } },
                {
                    "query": {
                        "rows": [
                            { "albumId": 2, "title": "Balls to the Wall" },
                            { "albumId": 3, "title": "Restless and Wild" }
                        ]
                    }
                },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
                { "query": { "rows": [] } },
            ]
        }))?;

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_value(pipeline).unwrap());
                Ok(mock_stream(vec![Ok(from_document(doc! {
                    "rows": [
                        {
                            "query": {
                                "rows": [
                                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                                    { "albumId": 4, "title": "Let There Be Rock" }
                                ]
                            }
                        },
                        {
                            "query": {
                                "rows": []
                            }
                        },
                        {
                            "query": {
                                "rows": [
                                    { "albumId": 2, "title": "Balls to the Wall" },
                                    { "albumId": 3, "title": "Restless and Wild" }
                                ]
                            }
                        },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                        { "query": { "rows": [] } },
                    ],
                })?)]))
            });

        let result = execute_query_request(&collection, query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }
}
