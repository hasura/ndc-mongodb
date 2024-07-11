use http::StatusCode;
use mongodb_agent_common::interface_types::{ErrorResponse, MongoAgentError};
use ndc_sdk::{
    connector::{ExplainError, QueryError},
    models,
};
use serde_json::Value;

pub fn mongo_agent_error_to_query_error(error: MongoAgentError) -> QueryError {
    if let MongoAgentError::NotImplemented(e) = error {
        return QueryError::UnsupportedOperation(error_response(e.to_owned()));
    }
    let (status, err) = error.status_and_error_response();
    match status {
        StatusCode::BAD_REQUEST => QueryError::UnprocessableContent(convert_error_response(err)),
        _ => QueryError::Other(Box::new(error), Value::Object(Default::default())),
    }
}

pub fn mongo_agent_error_to_explain_error(error: MongoAgentError) -> ExplainError {
    if let MongoAgentError::NotImplemented(e) = error {
        return ExplainError::UnsupportedOperation(error_response(e.to_owned()));
    }
    let (status, err) = error.status_and_error_response();
    match status {
        StatusCode::BAD_REQUEST => ExplainError::UnprocessableContent(convert_error_response(err)),
        _ => ExplainError::Other(Box::new(error), Value::Object(Default::default())),
    }
}

pub fn error_response(message: String) -> models::ErrorResponse {
    models::ErrorResponse {
        message,
        details: serde_json::Value::Object(Default::default()),
    }
}

pub fn convert_error_response(err: ErrorResponse) -> models::ErrorResponse {
    models::ErrorResponse {
        message: err.message,
        details: Value::Object(err.details.unwrap_or_default().into_iter().collect()),
    }
}
