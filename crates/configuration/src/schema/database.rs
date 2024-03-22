use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use mongodb_support::BsonScalarType;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    pub name: String,
    /// The name of a type declared in `objectTypes` that describes the fields of this collection.
    /// The type name may be the same as the collection name.
    pub r#type: String,
    #[serde(default)]
    pub description: Option<String>,
}

/// The type of values that a column, field, or argument may take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Type {
    /// One of the predefined BSON scalar types
    Scalar(BsonScalarType),
    /// The name of an object type declared in `objectTypes`
    Object(String),
    ArrayOf(Box<Type>),
    /// A nullable form of any of the other types
    Nullable(Box<Type>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectType {
    pub name: String,
    pub fields: Vec<ObjectField>,
    #[serde(default)]
    pub description: Option<String>,
}

/// Information about an object type field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectField {
    pub name: String,
    pub r#type: Type,
    #[serde(default)]
    pub description: Option<String>,
}

impl ObjectField {
    pub fn new(name: &str, r#type: Type) -> Self {
        ObjectField {
            name: name.to_owned(),
            r#type,
            description: Default::default(),
        }
    }
}
