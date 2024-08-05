use mongodb::bson::Bson;
use thiserror::Error;

use crate::{interface_types::MongoAgentError, query::serialization::JsonToBsonError};

#[derive(Debug, Error)]
pub enum ProcedureError {
    #[error("error parsing argument \"{}\": {}", .argument_name, .error)]
    ErrorParsingArgument {
        argument_name: String,
        #[source]
        error: JsonToBsonError,
    },

    #[error("error parsing predicate argument \"{}\": {}", .argument_name, .error)]
    ErrorParsingPredicate {
        argument_name: String,
        #[source]
        error: Box<MongoAgentError>,
    },

    #[error("error executing mongodb command: {0}")]
    ExecutionError(#[from] mongodb::error::Error),

    #[error("a required argument was not provided, \"{0}\"")]
    MissingArgument(ndc_models::ArgumentName),

    #[error("found a non-string argument, {0}, in a string context - if you want to use a non-string argument it must be the only thing in the string with no white space around the curly braces")]
    NonStringInStringContext(ndc_models::ArgumentName),

    #[error("object keys must be strings, but got: \"{0}\"")]
    NonStringKey(Bson),
}
