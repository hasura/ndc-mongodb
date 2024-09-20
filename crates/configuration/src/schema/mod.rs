use std::collections::BTreeMap;

use ref_cast::RefCast as _;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use mongodb_support::BsonScalarType;

use crate::{MongoScalarType, WithName, WithNameRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// The name of a type declared in `objectTypes` that describes the fields of this collection.
    /// The type name may be the same as the collection name.
    pub r#type: ndc_models::ObjectTypeName,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// The type of values that a column, field, or argument may take.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Type {
    /// Any BSON value, represented as Extended JSON.
    /// To be used when we don't have any more information
    /// about the types of values that a column, field or argument can take.
    /// Also used when we unifying two incompatible types in schemas derived
    /// from sample documents.
    ExtendedJSON,
    /// One of the predefined BSON scalar types
    Scalar(BsonScalarType),
    /// The name of an object type declared in `objectTypes`
    Object(String),
    ArrayOf(Box<Type>),
    /// A nullable form of any of the other types
    Nullable(Box<Type>),
    /// A predicate type for a given object type
    #[serde(rename_all = "camelCase")]
    Predicate {
        /// The object type name
        object_type_name: ndc_models::ObjectTypeName,
    },
}

impl Type {
    pub fn normalize_type(self) -> Type {
        match self {
            Type::ExtendedJSON => Type::ExtendedJSON,
            Type::Scalar(s) => Type::Scalar(s),
            Type::Object(o) => Type::Object(o),
            Type::Predicate { object_type_name } => Type::Predicate { object_type_name },
            Type::ArrayOf(a) => Type::ArrayOf(Box::new((*a).normalize_type())),
            Type::Nullable(n) => match *n {
                Type::ExtendedJSON => Type::ExtendedJSON,
                Type::Scalar(BsonScalarType::Null) => Type::Scalar(BsonScalarType::Null),
                Type::Nullable(t) => Type::Nullable(t).normalize_type(),
                t => Type::Nullable(Box::new(t.normalize_type())),
            },
        }
    }

    pub fn make_nullable(self) -> Type {
        match self {
            Type::ExtendedJSON => Type::ExtendedJSON,
            Type::Nullable(t) => Type::Nullable(t),
            Type::Scalar(BsonScalarType::Null) => Type::Scalar(BsonScalarType::Null),
            t => Type::Nullable(Box::new(t)),
        }
    }
}

impl From<Type> for ndc_models::Type {
    fn from(t: Type) -> Self {
        fn map_normalized_type(t: Type) -> ndc_models::Type {
            match t {
                // ExtendedJSON can respresent any BSON value, including null, so it is always nullable
                Type::ExtendedJSON => ndc_models::Type::Nullable {
                    underlying_type: Box::new(ndc_models::Type::Named {
                        name: mongodb_support::EXTENDED_JSON_TYPE_NAME.to_owned().into(),
                    }),
                },
                Type::Scalar(t) => ndc_models::Type::Named {
                    name: t.graphql_name().to_owned().into(),
                },
                Type::Object(t) => ndc_models::Type::Named {
                    name: t.clone().into(),
                },
                Type::ArrayOf(t) => ndc_models::Type::Array {
                    element_type: Box::new(map_normalized_type(*t)),
                },
                Type::Nullable(t) => ndc_models::Type::Nullable {
                    underlying_type: Box::new(map_normalized_type(*t)),
                },
                Type::Predicate { object_type_name } => {
                    ndc_models::Type::Predicate { object_type_name }
                }
            }
        }
        map_normalized_type(t.normalize_type())
    }
}

impl From<ndc_models::Type> for Type {
    fn from(t: ndc_models::Type) -> Self {
        match t {
            ndc_models::Type::Named { name } => {
                let scalar_type_name = ndc_models::ScalarTypeName::ref_cast(&name);
                match MongoScalarType::try_from(scalar_type_name) {
                    Ok(MongoScalarType::Bson(scalar_type)) => Type::Scalar(scalar_type),
                    Ok(MongoScalarType::ExtendedJSON) => Type::ExtendedJSON,
                    Err(_) => Type::Object(name.to_string()),
                }
            }
            ndc_models::Type::Nullable { underlying_type } => {
                Type::Nullable(Box::new(Self::from(*underlying_type)))
            }
            ndc_models::Type::Array { element_type } => {
                Type::ArrayOf(Box::new(Self::from(*element_type)))
            }
            ndc_models::Type::Predicate { object_type_name } => {
                Type::Predicate { object_type_name }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ObjectType {
    pub fields: BTreeMap<ndc_models::FieldName, ObjectField>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl ObjectType {
    pub fn named_fields(
        &self,
    ) -> impl Iterator<Item = WithNameRef<'_, ndc_models::FieldName, ObjectField>> {
        self.fields
            .iter()
            .map(|(name, field)| WithNameRef::named(name, field))
    }

    pub fn into_named_fields(
        self,
    ) -> impl Iterator<Item = WithName<ndc_models::FieldName, ObjectField>> {
        self.fields
            .into_iter()
            .map(|(name, field)| WithName::named(name, field))
    }
}

impl From<ObjectType> for ndc_models::ObjectType {
    fn from(object_type: ObjectType) -> Self {
        ndc_models::ObjectType {
            description: object_type.description,
            fields: object_type
                .fields
                .into_iter()
                .map(|(name, field)| (name, field.into()))
                .collect(),
        }
    }
}

impl From<ndc_models::ObjectType> for ObjectType {
    fn from(object_type: ndc_models::ObjectType) -> Self {
        ObjectType {
            description: object_type.description,
            fields: object_type
                .fields
                .into_iter()
                .map(|(name, field)| (name, field.into()))
                .collect(),
        }
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

impl From<ObjectField> for ndc_models::ObjectField {
    fn from(field: ObjectField) -> Self {
        ndc_models::ObjectField {
            description: field.description,
            r#type: field.r#type.into(),
            arguments: BTreeMap::new(),
        }
    }
}

impl From<ndc_models::ObjectField> for ObjectField {
    fn from(field: ndc_models::ObjectField) -> Self {
        ObjectField {
            description: field.description,
            r#type: field.r#type.into(),
        }
    }
}
