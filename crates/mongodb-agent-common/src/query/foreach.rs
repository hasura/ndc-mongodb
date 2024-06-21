use anyhow::anyhow;
use configuration::MongoScalarType;
use itertools::Itertools as _;
use mongodb::bson::{self, doc, Bson};
use ndc_query_plan::VariableSet;

use super::pipeline::pipeline_for_non_foreach;
use super::query_level::QueryLevel;
use super::query_variable_name::query_variable_name;
use super::serialization::json_to_bson;
use super::QueryTarget;
use crate::mongo_query_plan::{MongoConfiguration, QueryPlan, Type, VariableTypes};
use crate::mongodb::Selection;
use crate::{
    interface_types::MongoAgentError,
    mongodb::{Pipeline, Stage},
};

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Produces a complete MongoDB pipeline for a query request that includes variable sets.
pub fn pipeline_for_foreach(
    request_variable_sets: &[VariableSet],
    config: &MongoConfiguration,
    query_request: &QueryPlan,
) -> Result<Pipeline> {
    let target = QueryTarget::for_request(config, query_request);

    let variable_sets =
        variable_sets_to_bson(request_variable_sets, &query_request.variable_types)?;

    let variable_names = variable_sets
        .iter()
        .flat_map(|variable_set| variable_set.keys());
    let bindings: bson::Document = variable_names
        .map(|name| (name.to_owned(), format!("${name}").into()))
        .collect();

    let variable_sets_stage = Stage::Documents(variable_sets);

    let query_pipeline = pipeline_for_non_foreach(config, query_request, QueryLevel::Top)?;

    let lookup_stage = Stage::Lookup {
        from: target.input_collection().map(ToString::to_string),
        local_field: None,
        foreign_field: None,
        r#let: Some(bindings),
        pipeline: Some(query_pipeline),
        r#as: "query".to_string(),
    };

    let selection = if query_request.query.has_aggregates() {
        Stage::ReplaceWith(Selection(doc! {
            "aggregates": { "$getField": { "input": { "$first": "$query" }, "field": "aggregates" } },
            "rows": { "$getField": { "input": { "$first": "$query" }, "field": "rows" } },
        }))
    } else {
        Stage::ReplaceWith(Selection(doc! {
            "rows": "$query"
        }))
    };

    Ok(Pipeline {
        stages: vec![variable_sets_stage, lookup_stage, selection],
    })
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
    name: &'a str,
    value: &'a serde_json::Value,
    variable_types: impl IntoIterator<Item = &'a Option<Type>> + 'a,
) -> impl Iterator<Item = Result<(String, Bson)>> + 'a {
    variable_types.into_iter().map(|t| {
        let resolved_type = match t {
            None => &Type::Scalar(MongoScalarType::ExtendedJSON),
            Some(t) => t,
        };
        let variable_name = query_variable_name(name, resolved_type);
        let bson_value = json_to_bson(resolved_type, value.clone())
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
                                "count": { "$getField": {
                                    "field": "result",
                                    "input": { "$first": { "$getField": { "$literal": "count" } } }
                                } },
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
            .row_set(
                row_set()
                    .aggregates([("count", json!({ "$numberInt": "2" }))])
                    .rows([
                        [
                            ("albumId", json!(1)),
                            ("title", json!("For Those About To Rock We Salute You")),
                        ],
                        [("albumId", json!(4)), ("title", json!("Let There Be Rock"))],
                    ]),
            )
            .row_set(
                row_set()
                    .aggregates([("count", json!({ "$numberInt": "2" }))])
                    .rows([
                        [("albumId", json!(2)), ("title", json!("Balls to the Wall"))],
                        [("albumId", json!(3)), ("title", json!("Restless and Wild"))],
                    ]),
            )
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
