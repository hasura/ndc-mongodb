use std::collections::BTreeMap;

use indent::indent_all_by;
use ndc_models as ndc;
use thiserror::Error;

use super::unify_relationship_references::RelationshipUnificationError;

#[derive(Debug, Error)]
pub enum QueryPlanError {
    #[error("error parsing predicate: {}", .0)]
    ErrorParsingPredicate(#[source] serde_json::Error),

    #[error("expected an array at path {}", path.join("."))]
    ExpectedArray { path: Vec<String> },

    #[error("expected an object at path {}", path.join("."))]
    ExpectedObject { path: Vec<String> },

    #[error("unknown arguments: {}", .0.join(", "))]
    ExcessArguments(Vec<ndc::ArgumentName>),

    #[error("some arguments are invalid:\n{}", format_errors(.0))]
    InvalidArguments(BTreeMap<ndc::ArgumentName, QueryPlanError>),

    #[error("missing arguments: {}", .0.join(", "))]
    MissingArguments(Vec<ndc::ArgumentName>),

    #[error("not implemented: {}", .0)]
    NotImplemented(String),

    #[error("{0}")]
    RelationshipUnification(#[from] RelationshipUnificationError),

    #[error("The target of the query, {0}, is a function whose result type is not an object type")]
    RootTypeIsNotObject(String),

    #[error("{0}")]
    TypeMismatch(String),

    #[error("found predicate argument in a value-only context")]
    UnexpectedPredicate,

    #[error("Unknown comparison operator, \"{0}\"")]
    UnknownComparisonOperator(ndc::ComparisonOperatorName),

    #[error("Unknown scalar type, \"{0}\"")]
    UnknownScalarType(ndc::ScalarTypeName),

    #[error("Unknown object type, \"{0}\"")]
    UnknownObjectType(String),

    #[error(
        "Unknown field \"{field_name}\"{}{}",
        in_object_type(object_type.as_ref()),
        at_path(path)
    )]
    UnknownObjectTypeField {
        object_type: Option<ndc::ObjectTypeName>,
        field_name: ndc::FieldName,
        path: Vec<String>,
    },

    #[error("Unknown collection, \"{0}\"")]
    UnknownCollection(String),

    #[error("Unknown procedure, \"{0}\"")]
    UnknownProcedure(String),

    #[error("Unknown relationship, \"{relationship_name}\"{}", at_path(path))]
    UnknownRelationship {
        relationship_name: String,
        path: Vec<String>,
    },

    #[error("Unknown aggregate function, \"{aggregate_function}\"")]
    UnknownAggregateFunction {
        aggregate_function: ndc::AggregateFunctionName,
    },

    #[error("Query referenced a function, \"{0}\", but it has not been defined")]
    UnspecifiedFunction(ndc::FunctionName),

    #[error("Query referenced a relationship, \"{0}\", but did not include relation metadata in `collection_relationships`")]
    UnspecifiedRelation(ndc::RelationshipName),

    #[error("Expected field {field_name} of object {} to be an object type. Got {got}.", parent_type.clone().map(|n| n.to_string()).unwrap_or("".to_owned()))]
    ExpectedObjectTypeAtField {
        parent_type: Option<ndc::ObjectTypeName>,
        field_name: ndc::FieldName,
        got: String,
    },
}

fn at_path(path: &[String]) -> String {
    if path.is_empty() {
        "".to_owned()
    } else {
        format!(" at path {}", path.join("."))
    }
}

fn in_object_type(type_name: Option<&ndc::ObjectTypeName>) -> String {
    match type_name {
        Some(name) => format!(" in object type \"{name}\""),
        None => "".to_owned(),
    }
}

fn format_errors(errors: &BTreeMap<ndc_models::ArgumentName, impl ToString>) -> String {
    errors
        .iter()
        .map(|(name, error)| format!("  {name}:\n{}", indent_all_by(4, error.to_string())))
        .collect::<Vec<_>>()
        .join("\n")
}
