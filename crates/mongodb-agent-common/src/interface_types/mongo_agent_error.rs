use std::{
    borrow::Cow,
    fmt::{self, Display},
};

use http::StatusCode;
use mongodb::bson;
use ndc_query_plan::QueryPlanError;
use thiserror::Error;

use crate::{mongo_query_plan::Dimension, procedure::ProcedureError, query::QueryResponseError};

/// A superset of the DC-API `AgentError` type. This enum adds error cases specific to the MongoDB
/// agent.
#[derive(Debug, Error)]
pub enum MongoAgentError {
    BadCollectionSchema(Box<(String, bson::Bson, bson::de::Error)>), // boxed to avoid an excessively-large stack value
    BadQuery(anyhow::Error),
    InvalidGroupDimension(Dimension),
    InvalidVariableName(String),
    InvalidScalarTypeName(String),
    MongoDB(#[from] mongodb::error::Error),
    MongoDBDeserialization(#[from] mongodb::bson::de::Error),
    MongoDBSerialization(#[from] mongodb::bson::ser::Error),
    MongoDBSupport(#[from] mongodb_support::error::Error),
    NotImplemented(Cow<'static, str>),
    Procedure(#[from] ProcedureError),
    QueryPlan(#[from] QueryPlanError),
    ResponseSerialization(#[from] QueryResponseError),
    Serialization(serde_json::Error),
    UnknownAggregationFunction(String),
    UnspecifiedRelation(String),
    AdHoc(#[from] anyhow::Error),
}

use MongoAgentError::*;

impl MongoAgentError {
    pub fn status_and_error_response(&self) -> (StatusCode, ErrorResponse) {
        match self {
            BadCollectionSchema(boxed_details) => {
                let (collection_name, schema, err) = &**boxed_details;
                (
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
                )
            },
            BadQuery(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(&err)),
            InvalidGroupDimension(dimension) => (
            StatusCode::BAD_REQUEST, ErrorResponse::new(&format!("Cannot express grouping dimension as a MongoDB query document expression: {dimension:?}"))
        ),
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
            Procedure(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(err)),
            QueryPlan(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(err)),
            ResponseSerialization(err) => (StatusCode::BAD_REQUEST, ErrorResponse::new(err)),
            Serialization(err) => (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::new(&err)),
            UnknownAggregationFunction(function) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Unknown aggregation function, {function}")),
            ),
            UnspecifiedRelation(relation) => (
                StatusCode::BAD_REQUEST,
                ErrorResponse::new(&format!("Query referenced a relationship, \"{relation}\", but did not include relation metadata in `table_relationships`"))
            ),
            AdHoc(err) => (StatusCode::INTERNAL_SERVER_ERROR, ErrorResponse::new(&err)),
        }
    }
}

impl Display for MongoAgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (_, err) = self.status_and_error_response();
        write!(f, "{}", err.message)
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct ErrorResponse {
    pub details: Option<::std::collections::HashMap<String, serde_json::Value>>,
    pub message: String,
    pub r#type: Option<ErrorResponseType>,
}

impl ErrorResponse {
    pub fn new<T>(message: &T) -> ErrorResponse
    where
        T: Display + ?Sized,
    {
        ErrorResponse {
            details: None,
            message: format!("{message}"),
            r#type: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ErrorResponseType {
    UncaughtError,
    MutationConstraintViolation,
    MutationPermissionCheckFailure,
}

impl Display for ErrorResponseType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UncaughtError => f.write_str("uncaught-error"),
            Self::MutationConstraintViolation => f.write_str("mutation-constraint-violation"),
            Self::MutationPermissionCheckFailure => {
                f.write_str("mutation-permission-check-failure")
            }
        }
    }
}

impl Default for ErrorResponseType {
    fn default() -> ErrorResponseType {
        Self::UncaughtError
    }
}
