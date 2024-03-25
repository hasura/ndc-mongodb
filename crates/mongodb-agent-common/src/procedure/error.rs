use configuration::schema::Type;
use mongodb::bson::{self, Bson};
use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum ProcedureError {
    #[error("error converting parsing argument as extjson: {0}")]
    ExtJsonConversionError(#[from] bson::extjson::de::Error),

    #[error("invalid argument type: {0}")]
    InvalidArgumentType(#[from] mongodb_support::error::Error),

    #[error("a required argument was not provided, \"{0}\"")]
    MissingArgument(String),

    #[error("found a non-string argument, {0}, in a string context - if you want to use a non-string argument it must be the only thing in the string with no white space around the curly braces")]
    NonStringInStringContext(String),

    #[error("object keys must be strings, but got: \"{0}\"")]
    NonStringKey(Bson),

    #[error("argument type, \"{0:?}\", does not match parameter type, \"{1:?}\"")]
    TypeMismatch(Type, Type),

    #[error("an argument was provided for an undefined paremeter, \"{0}\"")]
    UnknownParameter(String),
}
