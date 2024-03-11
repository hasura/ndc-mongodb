use ndc_sdk::connector::{ExplainError, QueryError};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ConversionError {
    #[error("The connector does not yet support {0}")]
    NotImplemented(&'static str),

    #[error("{0}")]
    TypeMismatch(String),

    #[error("Unknown comparison operator, \"{0}\"")]
    UnknownComparisonOperator(String),

    #[error("Unknown scalar type, \"{0}\"")]
    UnknownScalarType(String),

    #[error("Query referenced a function, \"{0}\", but it has not been defined")]
    UnspecifiedFunction(String),

    #[error("Query referenced a relationship, \"{0}\", but did not include relation metadata in `collection_relationships`")]
    UnspecifiedRelation(String),
}

impl From<ConversionError> for QueryError {
    fn from(error: ConversionError) -> Self {
        match error {
            ConversionError::NotImplemented(e) => QueryError::UnsupportedOperation(e.to_owned()),
            e => QueryError::InvalidRequest(e.to_string()),
        }
    }
}

impl From<ConversionError> for ExplainError {
    fn from(error: ConversionError) -> Self {
        match error {
            ConversionError::NotImplemented(e) => ExplainError::UnsupportedOperation(e.to_owned()),
            e => ExplainError::InvalidRequest(e.to_string()),
        }
    }
}
