use std::collections::HashMap;

use configuration::{native_query::NativeQuery, Configuration};
use dc_api_types::{Argument, QueryRequest, VariableSet};
use itertools::Itertools as _;

use crate::{
    interface_types::MongoAgentError,
    mongodb::{Pipeline, Stage},
    mutation::{interpolated_command, MutationError},
};

use super::{arguments::resolve_arguments, query_target::QueryTarget};

/// Returns either the pipeline defined by a native query with variable bindings for arguments, or
/// an empty pipeline if the query request target is not a native query
pub fn pipeline_for_native_query(
    config: &Configuration,
    variables: Option<&VariableSet>,
    query_request: &QueryRequest,
) -> Result<Pipeline, MongoAgentError> {
    match QueryTarget::for_request(config, query_request) {
        QueryTarget::Collection(_) => Ok(Pipeline::empty()),
        QueryTarget::NativeQuery {
            native_query,
            arguments,
            ..
        } => make_pipeline(config, variables, native_query, arguments),
    }
}

fn make_pipeline(
    config: &Configuration,
    variables: Option<&VariableSet>,
    native_query: &NativeQuery,
    arguments: &HashMap<String, Argument>,
) -> Result<Pipeline, MongoAgentError> {
    let expressions = arguments
        .iter()
        .map(|(name, argument)| {
            Ok((
                name.to_owned(),
                argument_to_mongodb_expression(argument, variables)?,
            )) as Result<_, MongoAgentError>
        })
        .try_collect()?;

    let bson_arguments =
        resolve_arguments(&config.object_types, &native_query.arguments, expressions)
            .map_err(MutationError::UnresolvableArguments)?;

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

fn argument_to_mongodb_expression(
    argument: &Argument,
    variables: Option<&VariableSet>,
) -> Result<serde_json::Value, MongoAgentError> {
    match argument {
        Argument::Variable { name } => variables
            .and_then(|vs| vs.get(name))
            .ok_or_else(|| MongoAgentError::VariableNotDefined(name.to_owned()))
            .cloned(),
        Argument::Literal { value } => Ok(value.clone()),
        // TODO: Column references are needed for native queries that are a target of a relation.
        // MDB-106
        Argument::Column { .. } => Err(MongoAgentError::NotImplemented(
            "column references in native queries are not currently implemented",
        )),
    }
}

#[cfg(test)]
mod tests {
    use configuration::{
        native_query::{NativeQuery, NativeQueryRepresentation},
        schema::{ObjectField, ObjectType, Type},
        Configuration,
    };
    use dc_api_test_helpers::{column, query, query_request};
    use dc_api_types::Argument;
    use mongodb::bson::{bson, doc};
    use mongodb_support::BsonScalarType as S;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::{
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

        let object_types = [(
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
                            r#type: Type::Scalar(S::ObjectId),
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
        .into();

        let config = Configuration {
            native_queries: [("vectorSearch".to_owned(), native_query.clone())].into(),
            object_types,
            collections: Default::default(),
            functions: Default::default(),
            mutations: Default::default(),
            native_mutations: Default::default(),
            options: Default::default(),
        };

        let request = query_request()
            .target_with_arguments(
                ["vectorSearch"],
                [
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
                ],
            )
            .query(query().fields([
                column!("title": "String"),
                column!("genres": "String"),
                column!("year": "String"),
            ]))
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

        let expected_response = vec![
            doc! { "title": "Beau Geste", "year": 1926, "genres": ["Action", "Adventure", "Drama"] },
            doc! { "title": "For Heaven's Sake", "year": 1926, "genres": ["Action", "Comedy", "Romance"] },
        ];

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
