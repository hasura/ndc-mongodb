use std::collections::BTreeMap;

use configuration::native_query::NativeQuery;
use itertools::Itertools as _;
use mongodb::bson::Bson;
use ndc_models::ArgumentName;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{Argument, MongoConfiguration, QueryPlan},
    mongodb::{Pipeline, Stage},
    procedure::{interpolated_command, ProcedureError},
};

use super::{
    make_selector, query_target::QueryTarget, query_variable_name::query_variable_name,
    serialization::json_to_bson,
};

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
    arguments: &BTreeMap<ndc_models::ArgumentName, Argument>,
) -> Result<Pipeline, MongoAgentError> {
    let bson_arguments = arguments
        .iter()
        .map(|(name, argument)| {
            let bson = argument_to_mongodb_expression(name, argument.clone())?;
            Ok((name.clone(), bson)) as Result<_, MongoAgentError>
        })
        .try_collect()?;

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
    name: &ArgumentName,
    argument: Argument,
) -> Result<Bson, ProcedureError> {
    let bson = match argument {
        Argument::Literal {
            value,
            argument_type,
        } => json_to_bson(&argument_type, value).map_err(|error| {
            ProcedureError::ErrorParsingArgument {
                argument_name: name.to_string(),
                error,
            }
        })?,
        Argument::Variable {
            name,
            argument_type,
        } => {
            let mongodb_var_name = query_variable_name(&name, &argument_type);
            format!("$${mongodb_var_name}").into()
        }
        Argument::Predicate { expression } => make_selector(&expression)
            .map_err(|error| ProcedureError::ErrorParsingPredicate {
                argument_name: name.to_string(),
                error: Box::new(error),
            })?
            .into(),
    };
    Ok(bson)
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
                    "filter".into(),
                    ObjectField {
                        r#type: Type::ExtendedJSON,
                        description: None,
                    },
                ),
                (
                    "queryVector".into(),
                    ObjectField {
                        r#type: Type::ArrayOf(Box::new(Type::Scalar(S::Double))),
                        description: None,
                    },
                ),
                (
                    "numCandidates".into(),
                    ObjectField {
                        r#type: Type::Scalar(S::Int),
                        description: None,
                    },
                ),
                (
                    "limit".into(),
                    ObjectField {
                        r#type: Type::Scalar(S::Int),
                        description: None,
                    },
                ),
            ]
            .into(),
            result_document_type: "VectorResult".into(),
            object_types: [(
                "VectorResult".into(),
                ObjectType {
                    description: None,
                    fields: [
                        (
                            "_id".into(),
                            ObjectField {
                                r#type: Type::Scalar(S::ObjectId),
                                description: None,
                            },
                        ),
                        (
                            "title".into(),
                            ObjectField {
                                r#type: Type::Scalar(S::String),
                                description: None,
                            },
                        ),
                        (
                            "genres".into(),
                            ObjectField {
                                r#type: Type::ArrayOf(Box::new(Type::Scalar(S::String))),
                                description: None,
                            },
                        ),
                        (
                            "year".into(),
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
