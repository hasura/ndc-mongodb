use std::collections::{BTreeMap, BTreeSet, HashMap};

use configuration::schema::Type;
use mongodb::bson::{Bson, Document};
use ndc_models::{ArgumentName, FieldName, ObjectTypeName};
use thiserror::Error;

use super::type_constraint::{ObjectTypeConstraint, TypeConstraint, TypeVariable};

pub type Result<T> = std::result::Result<T, Error>;

// The URL for native query issues will be visible due to a wrapper around this error message in
// [crate::native_query::create].
const UNSUPPORTED_FEATURE_MESSAGE: &str = r#"For a list of currently-supported features see https://hasura.io/docs/3.0/connectors/mongodb/native-operations/supported-aggregation-pipeline-features/. Please file a bug report, and declare types for your native query by hand for the time being."#;

#[derive(Clone, Debug, Error, PartialEq)]
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

    #[error("Expected an array type, but got: {actual_type:?}")]
    ExpectedArray { actual_type: Type },

    #[error("Expected an array, but got: {actual_argument}")]
    ExpectedArrayExpressionArgument { actual_argument: Bson },

    #[error("Expected an object type, but got: {actual_type:?}")]
    ExpectedObject { actual_type: Type },

    #[error("Expected a path for the $unwind stage")]
    ExpectedStringPath(Bson),

    // This variant is not intended to be returned to the user - it is transformed with more
    // context in [super::PipelineTypeContext::into_types].
    #[error("Failed to unify: {unsolved_variables:?}")]
    FailedToUnify {
        unsolved_variables: Vec<TypeVariable>,
    },

    #[error("Cannot infer a result document type for pipeline because it does not produce documents. You might need to add a --collection flag to your command to specify an input collection for the query.")]
    IncompletePipeline,

    #[error("An object representing an expression must have exactly one field: {0}")]
    MultipleExpressionOperators(Document),

    #[error("Object type, {object_type}, does not have a field named {field_name}")]
    ObjectMissingField {
        object_type: ObjectTypeName,
        field_name: FieldName,
    },

    #[error("Type mismatch{}: {a} is not compatible with {b}", match context {
        Some(context) => format!(" in {}", context),
        None => String::new(),
    })]
    TypeMismatch {
        context: Option<String>,
        a: TypeConstraint,
        b: TypeConstraint,
    },

    #[error(
        "{}",
        unable_to_infer_types_message(*could_not_infer_return_type, problem_parameter_types)
    )]
    UnableToInferTypes {
        problem_parameter_types: Vec<ArgumentName>,
        could_not_infer_return_type: bool,

        // These fields are included here for internal debugging
        type_variables: HashMap<TypeVariable, BTreeSet<TypeConstraint>>,
        object_type_constraints: BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    },

    #[error("Error parsing a string in the aggregation pipeline: {0}")]
    UnableToParseReferenceShorthand(String),

    #[error("Type inference is not currently implemented for the query predicate operator, {0}. {UNSUPPORTED_FEATURE_MESSAGE}")]
    UnknownMatchDocumentOperator(String),

    #[error("Type inference is not currently implemented for the aggregation expression operator, {0}. {UNSUPPORTED_FEATURE_MESSAGE}")]
    UnknownAggregationOperator(String),

    #[error("Type inference is not currently implemented for{} stage number {} in your aggregation pipeline. {UNSUPPORTED_FEATURE_MESSAGE}", match stage_name { Some(name) => format!(" {name},"), None => "".to_string() }, stage_index + 1)]
    UnknownAggregationStage {
        stage_index: usize,
        stage_name: Option<&'static str>,
    },

    #[error("Native query input collection, \"{0}\", is not defined in the connector schema")]
    UnknownCollection(String),

    #[error("Unknown object type, \"{0}\"")]
    UnknownObjectType(String),

    #[error("{0}")]
    Other(String),

    #[error("Errors processing pipeline:\n\n{}", multiple_errors(.0))]
    Multiple(Vec<Error>),
}

fn unable_to_infer_types_message(
    could_not_infer_return_type: bool,
    problem_parameter_types: &[ArgumentName],
) -> String {
    let mut message = String::new();
    message += "Cannot infer types for this pipeline.\n";
    if !problem_parameter_types.is_empty() {
        message += "\nCould not infer types for these parameters:\n";
        for name in problem_parameter_types {
            message += &format!("- {name}\n");
        }
        message += "\nTry adding type annotations of the form: {{ parameter_name | [int!]! }}\n";
        message += "\nIf you added an annotation, and you are still seeing this error then the type you gave may not be compatible with the context where the parameter is used.\n";
    }
    if could_not_infer_return_type {
        message += "\nUnable to infer return type.";
        if !problem_parameter_types.is_empty() {
            message += " Adding type annotations to parameters may help.";
        }
        message += "\n";
    }
    message
}

fn multiple_errors(errors: &[Error]) -> String {
    let mut output = String::new();
    for error in errors {
        output += &format!("- {}\n", error);
    }
    output
}
