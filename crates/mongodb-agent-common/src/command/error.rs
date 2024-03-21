use mongodb::bson::{self, Bson};
use mongodb_support::BsonScalarType;
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum CommandError {
    #[error("error converting parsing argument as extjson: {0}")]
    ExtJsonConversionError(#[from] bson::extjson::de::Error),

    #[error("invalid argument type: {0}")]
    InvalidArgumentType(#[from] mongodb_support::error::Error),

    #[error("a required argument was not provided, \"{0}\"")]
    MissingArgument(String),

    #[error("object keys must be strings, but got: \"{0}\"")]
    NonStringKey(Bson),

    #[error("argument type, \"{0}\", does not match parameter type, \"{1}\"")]
    TypeMismatch(BsonScalarType, BsonScalarType),

    #[error("an argument was provided for an undefined paremeter, \"{0}\"")]
    UnknownParameter(String),
}
