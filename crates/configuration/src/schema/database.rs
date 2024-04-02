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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The type of values that a column, field, or argument may take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Type {
    /// Any BSON value. To be used when we don't have any more information
    /// about the types of values that a column, field or argument can take.
    /// Also used when we unifying two incompatible types in schemas derived
    /// from sample documents.
    Any,
    /// One of the predefined BSON scalar types
    Scalar(BsonScalarType),
    /// The name of an object type declared in `objectTypes`
    Object(String),
    ArrayOf(Box<Type>),
    /// A nullable form of any of the other types
    Nullable(Box<Type>),
}

impl Type {
    pub fn normalize_type(self) -> Type {
        match self {
            Type::Any => Type::Any,
            Type::Scalar(s) => Type::Scalar(s),
            Type::Object(o) => Type::Object(o),
            Type::ArrayOf(a) => Type::ArrayOf(Box::new((*a).normalize_type())),
            Type::Nullable(n) => match *n {
                Type::Any => Type::Any,
                Type::Scalar(BsonScalarType::Null) => Type::Scalar(BsonScalarType::Null),
                Type::Nullable(t) => Type::Nullable(t).normalize_type(),
                t => Type::Nullable(Box::new(t.normalize_type())),
            },
        }
    }

    pub fn make_nullable(self) -> Type {
        match self {
            Type::Any => Type::Any,
            Type::Nullable(t) => Type::Nullable(t),
            Type::Scalar(BsonScalarType::Null) => Type::Scalar(BsonScalarType::Null),
            t => Type::Nullable(Box::new(t)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectType {
    pub fields: BTreeMap<String, ObjectField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ObjectField {
    pub fn new(name: impl ToString, r#type: Type) -> (String, Self) {
        (
            name.to_string(),
            ObjectField {
                r#type,
                description: Default::default(),
            },
        )
    }
}
