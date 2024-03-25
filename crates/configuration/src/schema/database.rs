use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use mongodb_support::BsonScalarType;

use crate::{WithName, WithNameRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
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
    pub fields: BTreeMap<String, ObjectField>,
    #[serde(default)]
    pub description: Option<String>,
}

impl ObjectType {
    pub fn named_fields(&self) -> impl Iterator<Item = WithNameRef<'_, ObjectField>> {
        self.fields
            .iter()
            .map(|(name, field)| WithNameRef::named(name, field))
    }

    pub fn into_named_fields(self) -> impl Iterator<Item = WithName<ObjectField>> {
        self.fields
            .into_iter()
            .map(|(name, field)| WithName::named(name, field))
    }
}

/// Information about an object type field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectField {
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
