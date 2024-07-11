use mongodb::bson::Bson;
use thiserror::Error;

use crate::query::arguments::ArgumentError;

#[derive(Debug, Error)]
pub enum ProcedureError {
    #[error("error executing mongodb command: {0}")]
    ExecutionError(#[from] mongodb::error::Error),

    #[error("a required argument was not provided, \"{0}\"")]
    MissingArgument(ndc_models::ArgumentName),

    #[error("found a non-string argument, {0}, in a string context - if you want to use a non-string argument it must be the only thing in the string with no white space around the curly braces")]
    NonStringInStringContext(ndc_models::ArgumentName),

    #[error("object keys must be strings, but got: \"{0}\"")]
    NonStringKey(Bson),

    #[error("could not resolve arguments: {0}")]
    UnresolvableArguments(#[from] ArgumentError),
}
