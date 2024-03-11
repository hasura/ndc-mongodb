use http::StatusCode;
use mongodb_agent_common::interface_types::MongoAgentError;
use ndc_sdk::connector::{ExplainError, QueryError};

pub fn mongo_agent_error_to_query_error(error: MongoAgentError) -> QueryError {
    if let MongoAgentError::NotImplemented(e) = error {
        return QueryError::UnsupportedOperation(e.to_owned());
    }
    let (status, err) = error.status_and_error_response();
    match status {
        StatusCode::BAD_REQUEST => QueryError::InvalidRequest(err.message),
        _ => QueryError::Other(Box::new(error)),
    }
}

pub fn mongo_agent_error_to_explain_error(error: MongoAgentError) -> ExplainError {
    if let MongoAgentError::NotImplemented(e) = error {
        return ExplainError::UnsupportedOperation(e.to_owned());
    }
    let (status, err) = error.status_and_error_response();
    match status {
        StatusCode::BAD_REQUEST => ExplainError::InvalidRequest(err.message),
        _ => ExplainError::Other(Box::new(error)),
    }
}
