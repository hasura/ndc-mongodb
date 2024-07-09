use std::collections::BTreeMap;

use configuration::native_query::NativeQuery;
use itertools::Itertools as _;
use ndc_models::Argument;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{MongoConfiguration, QueryPlan},
    mongodb::{Pipeline, Stage},
    procedure::{interpolated_command, ProcedureError},
};

use super::{arguments::resolve_arguments, query_target::QueryTarget};

/// Returns either the pipeline defined by a native query with variable bindings for arguments, or
/// an empty pipeline if the query request target is not a native query
pub fn pipeline_for_native_query(
    config: &MongoConfiguration,
    query_request: &QueryPlan,
) -> Result<Pipeline, MongoAgentError> {
    match QueryTarget::for_request(config, query_request) {
        QueryTarget::Collection(_) => Ok(Pipeline::empty()),
        QueryTarget::NativeQuery {
            native_query,
            arguments,
            ..
        } => make_pipeline(native_query, arguments),
    }
}

fn make_pipeline(
    native_query: &NativeQuery,
    arguments: &BTreeMap<String, Argument>,
) -> Result<Pipeline, MongoAgentError> {
    let bson_arguments = resolve_arguments(&native_query.arguments, arguments.clone())
        .map_err(ProcedureError::UnresolvableArguments)?;

    // Replace argument placeholders with resolved expressions, convert document list to
    // a `Pipeline` value
    let stages = native_query
        .pipeline
        .iter()
        .map(|document| interpolated_command(document, &bson_arguments))
        .map_ok(Stage::Other)
        .try_collect()?;

    Ok(Pipeline::new(stages))
}

#[cfg(test)]
mod tests {
    use configuration::{
        native_query::NativeQueryRepresentation,
        schema::{ObjectField, ObjectType, Type},
        serialized::NativeQuery,
        Configuration,
    };
    use mongodb::bson::{bson, doc};
    use mongodb_support::BsonScalarType as S;
    use ndc_models::Argument;
    use ndc_test_helpers::{field, query, query_request, row_set};
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
        mongo_query_plan::MongoConfiguration,
        mongodb::test_helpers::mock_aggregate_response_for_pipeline, query::execute_query_request,
    };

    #[tokio::test]
    async fn executes_native_query() -> Result<(), anyhow::Error> {
        let native_query = NativeQuery {
            representation: NativeQueryRepresentation::Collection,
            input_collection: None,
            arguments: [
                (
                    "filter".to_string(),
                    ObjectField {
                        r#type: Type::ExtendedJSON,
                        description: None,
                    },
                ),
                (
                    "queryVector".to_string(),
                    ObjectField {
                        r#type: Type::ArrayOf(Box::new(Type::Scalar(S::Double))),
                        description: None,
                    },
                ),
                (
                    "numCandidates".to_string(),
                    ObjectField {
                        r#type: Type::Scalar(S::Int),
                        description: None,
                    },
                ),
                (
                    "limit".to_string(),
                    ObjectField {
                        r#type: Type::Scalar(S::Int),
                        description: None,
                    },
                ),
            ]
            .into(),
            result_document_type: "VectorResult".to_owned(),
            object_types: [(
                "VectorResult".to_owned(),
                ObjectType {
                    description: None,
                    fields: [
                        (
                            "_id".to_owned(),
                            ObjectField {
                                r#type: Type::Scalar(S::ObjectId),
                                description: None,
                            },
                        ),
                        (
                            "title".to_owned(),
                            ObjectField {
                                r#type: Type::Scalar(S::String),
                                description: None,
                            },
                        ),
                        (
                            "genres".to_owned(),
                            ObjectField {
                                r#type: Type::ArrayOf(Box::new(Type::Scalar(S::String))),
                                description: None,
                            },
                        ),
                        (
                            "year".to_owned(),
                            ObjectField {
                                r#type: Type::Scalar(S::Int),
                                description: None,
                            },
                        ),
                    ]
                    .into(),
                },
            )]
            .into(),
            pipeline: vec![doc! {
              "$vectorSearch": {
                "index": "movie-vector-index",
                "path": "plot_embedding",
                "filter": "{{ filter }}",
                "queryVector": "{{ queryVector }}",
                "numCandidates": "{{ numCandidates }}",
                "limit": "{{ limit }}"
              }
            }],
            description: None,
        };

        let config = MongoConfiguration(Configuration::validate(
            Default::default(),
            Default::default(),
            [("vectorSearch".into(), native_query)].into(),
            Default::default(),
        )?);

        let request = query_request()
            .collection("vectorSearch")
            .arguments([
                (
                    "filter",
                    Argument::Literal {
                        value: json!({
                            "$and": [
                                {
                                    "genres": {
                                        "$nin": [
                                            "Drama", "Western", "Crime"
                                        ],
                                        "$in": [
                                            "Action", "Adventure", "Family"
                                        ]
                                    }
                                }, {
                                    "year": { "$gte": 1960, "$lte": 2000 }
                                }
                            ]
                        }),
                    },
                ),
                (
                    "queryVector",
                    Argument::Literal {
                        value: json!([-0.020156775, -0.024996493, 0.010778184]),
                    },
                ),
                ("numCandidates", Argument::Literal { value: json!(200) }),
                ("limit", Argument::Literal { value: json!(10) }),
            ])
            .query(query().fields([field!("title"), field!("genres"), field!("year")]))
            .into();

        let expected_pipeline = bson!([
            {
                "$vectorSearch": {
                    "index": "movie-vector-index",
                    "path": "plot_embedding",
                    "filter": {
                        "$and": [
                            {
                                "genres": {
                                    "$nin": [
                                        "Drama", "Western", "Crime"
                                    ],
                                    "$in": [
                                        "Action", "Adventure", "Family"
                                    ]
                                }
                            }, {
                                "year": { "$gte": 1960, "$lte": 2000 }
                            }
                        ]
                    },
                    "queryVector": [-0.020156775, -0.024996493, 0.010778184],
                    "numCandidates": 200,
                    "limit": 10,
                }
            },
            {
                "$replaceWith": {
                    "title": { "$ifNull": ["$title", null] },
                    "year": { "$ifNull": ["$year", null] },
                    "genres": { "$ifNull": ["$genres", null] },
                }
            },
        ]);

        let expected_response = row_set()
            .rows([
                [
                    ("title", json!("Beau Geste")),
                    ("year", json!(1926)),
                    ("genres", json!(["Action", "Adventure", "Drama"])),
                ],
                [
                    ("title", json!("For Heaven's Sake")),
                    ("year", json!(1926)),
                    ("genres", json!(["Action", "Comedy", "Romance"])),
                ],
            ])
            .into_response();

        let db = mock_aggregate_response_for_pipeline(
            expected_pipeline,
            bson!([
                { "title": "Beau Geste", "year": 1926, "genres": ["Action", "Adventure", "Drama"] },
                { "title": "For Heaven's Sake", "year": 1926, "genres": ["Action", "Comedy", "Romance"] },
            ]),
        );

        let result = execute_query_request(db, &config, request).await?;
        assert_eq!(expected_response, result);
        Ok(())
    }
}
