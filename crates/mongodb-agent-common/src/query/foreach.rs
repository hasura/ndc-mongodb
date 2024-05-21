use mongodb::bson::{doc, Bson};
use ndc_query_plan::VariableSet;

use super::pipeline::pipeline_for_non_foreach;
use crate::mongo_query_plan::{MongoConfiguration, QueryPlan};
use crate::mongodb::Selection;
use crate::{
    interface_types::MongoAgentError,
    mongodb::{Pipeline, Stage},
};

const FACET_FIELD: &str = "__FACET__";

/// Produces a complete MongoDB pipeline for a foreach query.
///
/// For symmetry with [`super::execute_query_request::pipeline_for_query`] and
/// [`pipeline_for_non_foreach`] this function returns a pipeline paired with a value that
/// indicates whether the response requires post-processing in the agent.
pub fn pipeline_for_foreach(
    variable_sets: &Vec<VariableSet>,
    config: &MongoConfiguration,
    query_request: &QueryPlan,
) -> Result<Pipeline, MongoAgentError> {
    let pipelines: Vec<(String, Pipeline)> = variable_sets
        .into_iter()
        .enumerate()
        .map(|(index, variables)| {
            let pipeline = pipeline_for_non_foreach(config, Some(variables), query_request)?;
            Ok((facet_name(index), pipeline))
        })
        .collect::<Result<_, MongoAgentError>>()?;

    let selection = Selection(doc! {
        "row_sets": pipelines.iter().map(|(key, _)|
            Bson::String(format!("${key}")),
        ).collect::<Vec<_>>()
    });

    let queries = pipelines.into_iter().collect();

    Ok(Pipeline {
        stages: vec![Stage::Facet(queries), Stage::ReplaceWith(selection)],
    })
}

fn facet_name(index: usize) -> String {
    format!("{FACET_FIELD}_{index}")
}

#[cfg(test)]
mod tests {
    use dc_api_types::{BinaryComparisonOperator, ComparisonColumn, Field, Query, QueryRequest};
    use mongodb::bson::{bson, doc, Bson};
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json};

    use crate::{
        mongodb::test_helpers::mock_collection_aggregate_response_for_pipeline,
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

        let expected_pipeline = bson!([
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
                    "row_sets": [
                        "$__FACET___0",
                        "$__FACET___1",
                    ]
                },
            }
        ]);

        let expected_response = vec![doc! {
            "row_sets": [
                [
                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                    { "albumId": 4, "title": "Let There Be Rock" },
                ],
                [
                    { "albumId": 2, "title": "Balls to the Wall" },
                    { "albumId": 3, "title": "Restless and Wild" },
                ],
            ]
        }];

        let db = mock_collection_aggregate_response_for_pipeline(
            "tracks",
            expected_pipeline,
            bson!([{
                "row_sets": [
                    [
                        { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                        { "albumId": 4, "title": "Let There Be Rock" }
                    ],
                    [
                        { "albumId": 2, "title": "Balls to the Wall" },
                        { "albumId": 3, "title": "Restless and Wild" }
                    ],
                ],
            }]),
        );

        let result = execute_query_request(db, &Default::default(), query_request).await?;
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

        let expected_pipeline = bson!([
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
                    "row_sets": [
                        "$__FACET___0",
                        "$__FACET___1",
                    ]
                },
            }
        ]);

        let expected_response = vec![doc! {
            "row_sets": [
                {
                    "aggregates": {
                        "count": 2,
                    },
                    "rows": [
                        { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                        { "albumId": 4, "title": "Let There Be Rock" },
                    ]
                },
                {
                    "aggregates": {
                        "count": 2,
                    },
                    "rows": [
                        { "albumId": 2, "title": "Balls to the Wall" },
                        { "albumId": 3, "title": "Restless and Wild" },
                    ]
                },
            ]
        }];

        let db = mock_collection_aggregate_response_for_pipeline(
            "tracks",
            expected_pipeline,
            bson!([{
                "row_sets": [
                    {
                        "aggregates": {
                            "count": 2,
                        },
                        "rows": [
                            { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                            { "albumId": 4, "title": "Let There Be Rock" },
                        ]
                    },
                    {
                        "aggregates": {
                            "count": 2,
                        },
                        "rows": [
                            { "albumId": 2, "title": "Balls to the Wall" },
                            { "albumId": 3, "title": "Restless and Wild" },
                        ]
                    },
                ]
            }]),
        );

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_foreach_with_variables() -> Result<(), anyhow::Error> {
        let query_request = QueryRequest {
            foreach: None,
            variables: Some(
                (1..=12)
                    .map(|artist_id| [("artistId".to_owned(), json!(artist_id))].into())
                    .collect(),
            ),
            target: dc_api_types::Target::TTable {
                name: vec!["tracks".to_owned()],
                arguments: Default::default(),
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
                fields: Some(
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
                ),
                aggregates: None,
                aggregates_limit: None,
                limit: None,
                offset: None,
                order_by: None,
            }),
        };

        fn facet(artist_id: i32) -> Bson {
            bson!([
                { "$match": { "artistId": {"$eq": artist_id } } },
                { "$replaceWith": {
                    "albumId": { "$ifNull": ["$albumId", null] },
                    "title": { "$ifNull": ["$title", null] }
                } },
            ])
        }

        let expected_pipeline = bson!([
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
                    "row_sets": [
                        "$__FACET___0",
                        "$__FACET___1",
                        "$__FACET___2",
                        "$__FACET___3",
                        "$__FACET___4",
                        "$__FACET___5",
                        "$__FACET___6",
                        "$__FACET___7",
                        "$__FACET___8",
                        "$__FACET___9",
                        "$__FACET___10",
                        "$__FACET___11",
                    ]
                },
            }
        ]);

        let expected_response = vec![doc! {
            "row_sets": [
                [
                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                    { "albumId": 4, "title": "Let There Be Rock" }
                ],
                [],
                [
                    { "albumId": 2, "title": "Balls to the Wall" },
                    { "albumId": 3, "title": "Restless and Wild" }
                ],
                [],
                [],
                [],
                [],
                [],
                [],
                [],
                [],
            ]
        }];

        let db = mock_collection_aggregate_response_for_pipeline(
            "tracks",
            expected_pipeline,
            bson!([{
                "row_sets": [
                    [
                        { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                        { "albumId": 4, "title": "Let There Be Rock" }
                    ],
                    [],
                    [
                        { "albumId": 2, "title": "Balls to the Wall" },
                        { "albumId": 3, "title": "Restless and Wild" }
                    ],
                    [],
                    [],
                    [],
                    [],
                    [],
                    [],
                    [],
                    [],
                ],
            }]),
        );

        let result = execute_query_request(db, &Default::default(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }
}
