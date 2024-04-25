use ndc_sdk::connector::{ExplainError, QueryError};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ConversionError {
    #[error("The connector does not yet support {0}")]
    NotImplemented(&'static str),

    #[error("The target of the query, {0}, is a function whose result type is not an object type")]
    RootTypeIsNotObject(String),

    #[error("{0}")]
    TypeMismatch(String),

    #[error("Unknown comparison operator, \"{0}\"")]
    UnknownComparisonOperator(String),

    #[error("Unknown scalar type, \"{0}\"")]
    UnknownScalarType(String),

    #[error("Unknown object type, \"{0}\"")]
    UnknownObjectType(String),

    #[error(
        "Unknown field \"{field_name}\" in object type \"{object_type}\"{}",
        at_path(path)
    )]
    UnknownObjectTypeField {
        object_type: String,
        field_name: String,
        path: Vec<String>,
    },

    #[error("Unknown collection, \"{0}\"")]
    UnknownCollection(String),

    #[error("Unknown relationship, \"{relationship_name}\"{}", at_path(path))]
    UnknownRelationship {
        relationship_name: String,
        path: Vec<String>,
    },

    #[error(
        "Unknown aggregate function, \"{aggregate_function}\" in scalar type \"{scalar_type}\""
    )]
    UnknownAggregateFunction {
        scalar_type: String,
        aggregate_function: String,
    },

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

fn at_path(path: &Vec<String>) -> String {
    if path.is_empty() {
        "".to_owned()
    } else {
        format!(" at path {}", path.join("."))
    }
}
