use anyhow::anyhow;
use itertools::Itertools as _;
use mongodb::bson::{self, doc, Bson};
use mongodb_support::aggregate::{AggregateCommand, Pipeline, Selection, Stage};
use ndc_query_plan::VariableSet;

use super::pipeline::pipeline_for_non_foreach;
use super::query_level::QueryLevel;
use super::query_variable_name::query_variable_name;
use super::serialization::json_to_bson;
use super::QueryTarget;
use crate::interface_types::MongoAgentError;
use crate::mongo_query_plan::{MongoConfiguration, QueryPlan, Type, VariableTypes};

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Produces a complete MongoDB pipeline for a query request that includes variable sets.
pub fn command_for_foreach(
    request_variable_sets: &[VariableSet],
    config: &MongoConfiguration,
    query_request: &QueryPlan,
) -> Result<AggregateCommand> {
    let target = QueryTarget::for_request(config, query_request);

    let variable_sets =
        variable_sets_to_bson(request_variable_sets, &query_request.variable_types)?;

    let query_pipeline = pipeline_for_non_foreach(config, query_request, QueryLevel::Top)?;

    // If there are multiple variable sets we need to use sub-pipelines to fork the query for each
    // set. So we start the pipeline with a `$documents` stage to inject variable sets, and join
    // the target collection with a `$lookup` stage with the query pipeline as a sub-pipeline. But
    // if there is exactly one variable set then we can optimize away the `$lookup`. This is useful
    // because some aggregation operations, like `$vectorSearch`, are not allowed in sub-pipelines.
    Ok(if variable_sets.len() == 1 {
        // safety: we just checked the length of variable_sets
        let single_set = variable_sets.into_iter().next().unwrap();
        command_for_single_variable_set(single_set, target, query_pipeline)
    } else {
        command_for_multiple_variable_sets(query_request, variable_sets, target, query_pipeline)
    })
}

// Where "multiple" means either zero or more than 1
fn command_for_multiple_variable_sets(
    query_request: &QueryPlan,
    variable_sets: Vec<bson::Document>,
    target: QueryTarget<'_>,
    query_pipeline: Pipeline,
) -> AggregateCommand {
    let variable_names = variable_sets
        .iter()
        .flat_map(|variable_set| variable_set.keys());
    let bindings: bson::Document = variable_names
        .map(|name| (name.to_owned(), format!("${name}").into()))
        .collect();

    let variable_sets_stage = Stage::Documents(variable_sets);

    let lookup_stage = Stage::Lookup {
        from: target.input_collection().map(ToString::to_string),
        local_field: None,
        foreign_field: None,
        r#let: Some(bindings),
        pipeline: Some(query_pipeline),
        r#as: "query".to_string(),
    };

    let selection = if query_request.query.has_aggregates() && query_request.query.has_fields() {
        doc! {
            "aggregates": { "$getField": { "input": { "$first": "$query" }, "field": "aggregates" } },
            "rows": { "$getField": { "input": { "$first": "$query" }, "field": "rows" } },
        }
    } else if query_request.query.has_aggregates() {
        doc! {
            "aggregates": { "$getField": { "input": { "$first": "$query" }, "field": "aggregates" } },
        }
    } else {
        doc! {
            "rows": "$query"
        }
    };
    let selection_stage = Stage::ReplaceWith(Selection::new(selection));

    AggregateCommand {
        collection: None,
        pipeline: Pipeline {
            stages: vec![variable_sets_stage, lookup_stage, selection_stage],
        },
        let_vars: None,
    }
}

fn command_for_single_variable_set(
    variable_set: bson::Document,
    target: QueryTarget<'_>,
    query_pipeline: Pipeline,
) -> AggregateCommand {
    AggregateCommand {
        collection: target.input_collection().map(ToString::to_string),
        pipeline: query_pipeline,
        let_vars: Some(variable_set),
    }
}

fn variable_sets_to_bson(
    variable_sets: &[VariableSet],
    variable_types: &VariableTypes,
) -> Result<Vec<bson::Document>> {
    variable_sets
        .iter()
        .map(|variable_set| {
            variable_set
                .iter()
                .flat_map(|(variable_name, value)| {
                    let types = variable_types.get(variable_name);
                    variable_to_bson(variable_name, value, types.iter().copied().flatten())
                        .collect_vec()
                })
                .try_collect()
        })
        .try_collect()
}

/// It may be necessary to include a request variable in the MongoDB pipeline multiple times if it
/// requires different BSON serializations.
fn variable_to_bson<'a>(
    name: &'a ndc_models::VariableName,
    value: &'a serde_json::Value,
    variable_types: impl IntoIterator<Item = &'a Type> + 'a,
) -> impl Iterator<Item = Result<(String, Bson)>> + 'a {
    variable_types.into_iter().map(|variable_type| {
        let variable_name = query_variable_name(name, variable_type);
        let bson_value = json_to_bson(variable_type, value.clone())
            .map_err(|e| MongoAgentError::BadQuery(anyhow!(e)))?;
        Ok((variable_name, bson_value))
    })
}

#[cfg(test)]
mod tests {
    use configuration::Configuration;
    use itertools::Itertools as _;
    use mongodb::bson::{bson, doc};
    use ndc_test_helpers::{
        binop, collection, field, named_type, object_type, query, query_request, query_response,
        row_set, star_count_aggregate, target, variable,
    };
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        mongo_query_plan::MongoConfiguration,
        mongodb::test_helpers::mock_aggregate_response_for_pipeline,
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
                "$documents": [
                    { "artistId_int": 1 },
                    { "artistId_int": 2 },
                ],
            },
            {
                "$lookup": {
                    "from": "tracks",
                    "let": {
                        "artistId_int": "$artistId_int",
                    },
                    "as": "query",
                    "pipeline": [
                        { "$match": { "$expr": { "$eq": ["$artistId", "$$artistId_int"] } } },
                        { "$replaceWith": {
                            "albumId": { "$ifNull": ["$albumId", null] },
                            "title": { "$ifNull": ["$title", null] }
                        } },
                    ],
                },
            },
            {
                "$replaceWith": {
                    "rows": "$query",
                }
            },
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

        let db = mock_aggregate_response_for_pipeline(
            expected_pipeline,
            bson!([
                { "rows": [
                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                    { "albumId": 4, "title": "Let There Be Rock" }
                ] },
                { "rows": [
                    { "albumId": 2, "title": "Balls to the Wall" },
                    { "albumId": 3, "title": "Restless and Wild" }
                ] },
            ]),
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
                "$documents": [
                    { "artistId_int": 1 },
                    { "artistId_int": 2 },
                ]
            },
            {
                "$lookup": {
                    "from": "tracks",
                    "let": {
                        "artistId_int": "$artistId_int"
                    },
                    "as": "query",
                    "pipeline": [
                        { "$match": { "$expr": { "$eq": ["$artistId", "$$artistId_int"] } }},
                        { "$facet": {
                            "__ROWS__": [{ "$replaceWith": {
                                "albumId": { "$ifNull": ["$albumId", null] },
                                "title": { "$ifNull": ["$title", null] }
                            }}],
                            "count": [{ "$count": "result" }],
                        } },
                        { "$replaceWith": {
                            "aggregates": {
                                "count": {
                                    "$ifNull": [
                                        {
                                            "$getField": {
                                                "field": "result",
                                                "input": { "$first": { "$getField": { "$literal": "count" } } }
                                            }
                                        },
                                        0,
                                    ]
                                },
                            },
                            "rows": "$__ROWS__",
                        } },
                    ]
                }
            },
            {
                "$replaceWith": {
                    "aggregates": { "$getField": { "input": { "$first": "$query" }, "field": "aggregates" } },
                    "rows": { "$getField": { "input": { "$first": "$query" }, "field": "rows" } },
                }
            },
        ]);

        let expected_response = query_response()
            .row_set(row_set().aggregates([("count", json!(2))]).rows([
                [
                    ("albumId", json!(1)),
                    ("title", json!("For Those About To Rock We Salute You")),
                ],
                [("albumId", json!(4)), ("title", json!("Let There Be Rock"))],
            ]))
            .row_set(row_set().aggregates([("count", json!(2))]).rows([
                [("albumId", json!(2)), ("title", json!("Balls to the Wall"))],
                [("albumId", json!(3)), ("title", json!("Restless and Wild"))],
            ]))
            .build();

        let db = mock_aggregate_response_for_pipeline(
            expected_pipeline,
            bson!([
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
            ]),
        );

        let result = execute_query_request(db, &music_config(), query_request).await?;
        assert_eq!(result, expected_response);

        Ok(())
    }

    #[tokio::test]
    async fn executes_query_with_variables_and_aggregates_and_no_rows() -> Result<(), anyhow::Error>
    {
        let query_request = query_request()
            .collection("tracks")
            .query(
                query()
                    .aggregates([star_count_aggregate!("count")])
                    .predicate(binop("_eq", target!("artistId"), variable!(artistId))),
            )
            .variables([[("artistId", 1)], [("artistId", 2)]])
            .into();

        let expected_pipeline = bson!([
            {
                "$documents": [
                    { "artistId_int": 1 },
                    { "artistId_int": 2 },
                ]
            },
            {
                "$lookup": {
                    "from": "tracks",
                    "let": {
                        "artistId_int": "$artistId_int"
                    },
                    "as": "query",
                    "pipeline": [
                        { "$match": { "$expr": { "$eq": ["$artistId", "$$artistId_int"] } }},
                        { "$facet": {
                            "count": [{ "$count": "result" }],
                        } },
                        { "$replaceWith": {
                            "aggregates": {
                                "count": {
                                    "$ifNull": [
                                        {
                                            "$getField": {
                                                "field": "result",
                                                "input": { "$first": { "$getField": { "$literal": "count" } } }
                                            }
                                        },
                                        0,
                                    ]
                                },
                            },
                        } },
                    ]
                }
            },
            {
                "$replaceWith": {
                    "aggregates": { "$getField": { "input": { "$first": "$query" }, "field": "aggregates" } },
                }
            },
        ]);

        let expected_response = query_response()
            .row_set(row_set().aggregates([("count", json!(2))]))
            .row_set(row_set().aggregates([("count", json!(2))]))
            .build();

        let db = mock_aggregate_response_for_pipeline(
            expected_pipeline,
            bson!([
                {
                    "aggregates": {
                        "count": 2,
                    },
                },
                {
                    "aggregates": {
                        "count": 2,
                    },
                },
            ]),
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

        let expected_pipeline = bson!([
            {
                "$documents": (1..=12).map(|artist_id| doc! { "artistId_int": artist_id }).collect_vec(),
            },
            {
                "$lookup": {
                    "from": "tracks",
                    "let": {
                        "artistId_int": "$artistId_int"
                    },
                    "as": "query",
                    "pipeline": [
                        {
                            "$match": {
                                "$expr": { "$eq": ["$artistId", "$$artistId_int"] }
                            }
                        },
                        {
                            "$replaceWith": {
                                "albumId": { "$ifNull": ["$albumId", null] },
                                "title": { "$ifNull": ["$title", null] }
                            }
                        },
                    ]
                }
            },
            {
                "$replaceWith": {
                    "rows": "$query"
                }
            },
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

        let db = mock_aggregate_response_for_pipeline(
            expected_pipeline,
            bson!([
                { "rows": [
                    { "albumId": 1, "title": "For Those About To Rock We Salute You" },
                    { "albumId": 4, "title": "Let There Be Rock" }
                ] },
                { "rows": [] },
                { "rows": [
                    { "albumId": 2, "title": "Balls to the Wall" },
                    { "albumId": 3, "title": "Restless and Wild" }
                ] },
                { "rows": [] },
                { "rows": [] },
                { "rows": [] },
                { "rows": [] },
                { "rows": [] },
                { "rows": [] },
                { "rows": [] },
                { "rows": [] },
            ]),
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
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }
}
