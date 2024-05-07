use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The type of values that a column, field, or argument may take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Type<ScalarType> {
    Scalar(ScalarType),
    /// The name of an object type declared in `objectTypes`
    Object(String),
    ArrayOf(Box<Type<ScalarType>>),
    /// A nullable form of any of the other types
    Nullable(Box<Type<ScalarType>>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectType<ScalarType> {
    pub fields: BTreeMap<String, ObjectField<ScalarType>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Information about an object type field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectField<ScalarType> {
    pub r#type: Type<ScalarType>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}
