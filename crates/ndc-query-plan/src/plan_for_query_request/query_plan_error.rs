use thiserror::Error;

#[derive(Clone, Debug, Error)]
pub enum QueryPlanError {
    #[error("expected an array at path {}", path.join("."))]
    ExpectedArray { path: Vec<String> },

    #[error("expected an object at path {}", path.join("."))]
    ExpectedObject { path: Vec<String> },

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
        "Unknown field \"{field_name}\"{}{}",
        in_object_type(object_type.as_ref()),
        at_path(path)
    )]
    UnknownObjectTypeField {
        object_type: Option<String>,
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

    #[error("Unknown aggregate function, \"{aggregate_function}\"")]
    UnknownAggregateFunction { aggregate_function: String },

    #[error("Query referenced a function, \"{0}\", but it has not been defined")]
    UnspecifiedFunction(String),

    #[error("Query referenced a relationship, \"{0}\", but did not include relation metadata in `collection_relationships`")]
    UnspecifiedRelation(String),
}

fn at_path(path: &[String]) -> String {
    if path.is_empty() {
        "".to_owned()
    } else {
        format!(" at path {}", path.join("."))
    }
}

fn in_object_type(type_name: Option<&String>) -> String {
    match type_name {
        Some(name) => format!(" in object type \"{name}\""),
        None => "".to_owned(),
    }
}
