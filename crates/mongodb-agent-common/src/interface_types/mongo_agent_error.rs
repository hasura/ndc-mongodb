use std::fmt::{self, Display};

use axum::{response::IntoResponse, Json};
use dc_api_types::ErrorResponse;
use http::StatusCode;
use mongodb::bson;
use thiserror::Error;

/// A superset of the DC-API `AgentError` type. This enum adds error cases specific to the MongoDB
/// agent.
#[derive(Debug, Error)]
pub enum MongoAgentError {
    BadCollectionSchema(String, bson::Bson, bson::de::Error),
    BadQuery(anyhow::Error),
    InvalidVariableName(String),
    InvalidScalarTypeName(String),
    MongoDB(#[from] mongodb::error::Error),
    MongoDBDeserialization(#[from] mongodb::bson::de::Error),
    MongoDBSerialization(#[from] mongodb::bson::ser::Error),
    MongoDBSupport(#[from] mongodb_support::error::Error),
    NotImplemented(&'static str),
    Serialization(serde_json::Error),
    UnknownAggregationFunction(String),
    UnspecifiedRelation(String),
    VariableNotDefined(String),
    AdHoc(#[from] anyhow::Error),
    AgentError(#[from] dc_api::AgentError),
}

use MongoAgentError::*;

impl MongoAgentError {
    pub fn status_and_error_response(&self) -> (StatusCode, ErrorResponse) {
        match self {
            BadCollectionSchema(collection_name, schema, err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorResponse {
                    message: format!("Could not parse a collection validator: {err}"),
                    details: Some(
                        [
                            (
                                "collection_name".to_owned(),
                                serde_json::Value::String(collection_name.clone()),
                            ),
                            (
                                "collection_validator".to_owned(),
                                bson::from_bson::<serde_json::Value>(schema.clone())
                                    .unwrap_or_else(|err| {
                                        serde_json::Value::String(format!(
                                            "Failed to convert bson validator to json: {err}"
                                        ))
                                    }),
                            ),
                        ]
                        .into(),
                    ),
                    r#type: None,
                },
            ),
            BadQuery(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            InvalidVariableName(name) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Column identifier includes characters that are not permitted in a MongoDB variable name: {name}"))
            ),
            InvalidScalarTypeName(name) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Scalar value includes invalid type name: {name}"))
            ),
            MongoDB(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            MongoDBDeserialization(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            MongoDBSerialization(err) => {
                (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::new(&err))
            }
            MongoDBSupport(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            NotImplemented(missing_feature) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&format!("The MongoDB agent does not yet support {missing_feature}"))),
            Serialization(err) => (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::new(&err)),
            UnknownAggregationFunction(function) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Unknown aggregation function, {function}")),
            ),
            UnspecifiedRelation(relation) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Query referenced a relationship, \"{relation}\", but did not include relation metadata in `table_relationships`"))
            ),
            VariableNotDefined(variable_name) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Query referenced a variable, \"{variable_name}\", but it is not defined by the query request"))
            ),
            AdHoc(err) => (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::new(&err)),
            AgentError(err) => err.status_and_error_response(),
        }
    }
}

impl Display for MongoAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (_, err) = self.status_and_error_response();
        write!(f, "{}", err.message)
    }
}

impl IntoResponse for MongoAgentError {
    fn into_response(self) -> axum::response::Response {
        if cfg!(debug_assertions) {
            // Log certain errors in development only. The `debug_assertions` feature is present in
            // debug builds, which we use during development. It is not present in release builds.
            #[allow(clippy::single_match)]
            match &self {
                BadCollectionSchema(collection_name, collection_validator, err) => {
                    tracing::warn!(collection_name, ?collection_validator, error = %err, "error parsing collection validator")
                }
                _ => (),
            }
        }
        let (status, resp) = self.status_and_error_response();
        (status, Json(resp)).into_response()
    }
}
