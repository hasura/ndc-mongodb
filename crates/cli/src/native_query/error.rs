use configuration::schema::Type;
use mongodb::bson::{self, Bson};
use ndc_models::{FieldName, ObjectTypeName};
use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, Error)]
pub enum Error {
    #[error("Cannot infer a result type for an empty pipeline")]
    EmptyPipeline,

    #[error(
        "Expected {reference} to reference an array, but instead it references a {referenced_type:?}"
    )]
    ExpectedArrayReference {
        reference: Bson,
        referenced_type: Type,
    },

    #[error("Expected a path for the $unwind stage")]
    ExpectedStringPath(Bson),

    #[error(
        "Cannot infer a result document type for pipeline because it does not produce documents"
    )]
    IncompletePipeline,

    #[error("Object type, {object_type}, does not have a field named {field_name}")]
    ObjectMissingField {
        object_type: ObjectTypeName,
        field_name: FieldName,
    },

    #[error("Cannot infer a result type for this pipeline. But you can create a native query by writing the configuration file by hand.")]
    UnableToInferResultType,

    #[error("Error parsing a string in the aggregation pipeline: {0}")]
    UnableToParseReferenceShorthand(String),

    #[error("Type inference is not currently implemented for stage {stage_index} in the aggregation pipeline. Please file a bug report, and declare types for your native query by hand.\n\n{stage}")]
    UnknownAggregationStage {
        stage_index: usize,
        stage: bson::Document,
    },

    #[error("Native query input collection, \"{0}\", is not defined in the connector schema")]
    UnknownCollection(String),

    #[error("Unknown object type, \"{0}\"")]
    UnknownObjectType(String),
}
