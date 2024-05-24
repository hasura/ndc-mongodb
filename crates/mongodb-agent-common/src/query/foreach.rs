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
    variable_sets: &[VariableSet],
    config: &MongoConfiguration,
    query_request: &QueryPlan,
) -> Result<Pipeline, MongoAgentError> {
    let pipelines: Vec<(String, Pipeline)> = variable_sets
        .iter()
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
    use configuration::Configuration;
    use mongodb::bson::{bson, Bson};
    use ndc_test_helpers::{
        binop, collection, field, named_type, object_type, query, query_request, query_response,
        row_set, star_count_aggregate, target, variable,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        mongo_query_plan::MongoConfiguration,
        mongodb::test_helpers::mock_collection_aggregate_response_for_pipeline,
        query::execute_query_request::execute_query_request,
    };

    #[tokio::test]
    async fn executes_query_with_variables_and_fields() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("tracks")
            .query(
                query()
                    .fields([field!("albumId"), field!("title")])
                    .predicate(binop("_eq", target!("artistId"), variable!(artistId))),
            )
            .variables([[("artistId", json!(1))], [("artistId", json!(2))]])
            .into();

        let expected_pipeline = bson!([
            {
                "$facet": {
                    "__FACET___0": [
                        { "$match": { "artistId": { "$eq": 1 } } },
                        { "$replaceWith": {
                            "albumId": { "$ifNull": ["$albumId", null] },
                            "title": { "$ifNull": ["$title", null] }
                        } },
                    ],
                    "__FACET___1": [
                        { "$match": { "artistId": { "$eq": 2 } } },
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

        let expected_response = query_response()
            .row_set_rows([
                [
                    ("albumId", json!(1)),
                    ("title", json!("For Those About To Rock We Salute You")),
                ],
                [("albumId", json!(4)), ("title", json!("Let There Be Rock"))],
            ])
            .row_set_rows([
                [("albumId", json!(2)), ("title", json!("Balls to the Wall"))],
                [("albumId", json!(3)), ("title", json!("Restless and Wild"))],
            ])
            .build();

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

        let result = execute_query_request(db, &music_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_query_with_variables_and_aggregates() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("tracks")
            .query(
                query()
                    .aggregates([star_count_aggregate!("count")])
                    .fields([field!("albumId"), field!("title")])
                    .predicate(binop("_eq", target!("artistId"), variable!(artistId))),
            )
            .variables([[("artistId", 1)], [("artistId", 2)]])
            .into();

        let expected_pipeline = bson!([
            {
                "$facet": {
                    "__FACET___0": [
                        { "$match": { "artistId": {"$eq": 1 }}},
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
                        { "$match": { "artistId": {"$eq": 2 }}},
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

        let expected_response = query_response()
            .row_set(row_set().aggregates([("count", 2)]).rows([
                [
                    ("albumId", json!(1)),
                    ("title", json!("For Those About To Rock We Salute You")),
                ],
                [("albumId", json!(4)), ("title", json!("Let There Be Rock"))],
            ]))
            .row_set(row_set().aggregates([("count", 2)]).rows([
                [("albumId", json!(2)), ("title", json!("Balls to the Wall"))],
                [("albumId", json!(3)), ("title", json!("Restless and Wild"))],
            ]))
            .build();

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

        let result = execute_query_request(db, &music_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    #[tokio::test]
    async fn executes_request_with_more_than_ten_variable_sets() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .variables((1..=12).map(|artist_id| [("artistId", artist_id)]))
            .collection("tracks")
            .query(
                query()
                    .predicate(binop("_eq", target!("artistId"), variable!(artistId)))
                    .fields([field!("albumId"), field!("title")]),
            )
            .into();

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

        let expected_response = query_response()
            .row_set_rows([
                [
                    ("albumId", json!(1)),
                    ("title", json!("For Those About To Rock We Salute You")),
                ],
                [("albumId", json!(4)), ("title", json!("Let There Be Rock"))],
            ])
            .empty_row_set()
            .row_set_rows([
                [("albumId", json!(2)), ("title", json!("Balls to the Wall"))],
                [("albumId", json!(3)), ("title", json!("Restless and Wild"))],
            ])
            .empty_row_set()
            .empty_row_set()
            .empty_row_set()
            .empty_row_set()
            .empty_row_set()
            .empty_row_set()
            .empty_row_set()
            .empty_row_set()
            .build();

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

        let result = execute_query_request(db, &music_config(), query_request).await?;
        assert_eq!(expected_response, result);

        Ok(())
    }

    fn music_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("tracks")].into(),
            object_types: [(
                "tracks".into(),
                object_type([
                    ("albumId", named_type("Int")),
                    ("artistId", named_type("Int")),
                    ("title", named_type("String")),
                ]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_procedures: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }
}
