use std::collections::BTreeMap;

use configuration::MongoScalarType;
use deriving_via::DerivingVia;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};
use nonempty::NonEmpty;
use ref_cast::RefCast as _;

#[derive(DerivingVia)]
#[deriving(Copy, Debug, Eq, Hash)]
pub struct TypeVariable(u32);

impl TypeVariable {
    pub fn new(id: u32) -> Self {
        TypeVariable(id)
    }
}

/// A TypeConstraint is almost identical to a [configuration::schema::Type], except that
/// a TypeConstraint may reference type variables.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeConstraint {
    // Normal type stuff - except that composite types might include variables in their structure.
    ExtendedJSON,
    Scalar(BsonScalarType),
    Object(ObjectTypeName),
    ArrayOf(Box<TypeConstraint>),
    Nullable(Box<TypeConstraint>),
    Predicate {
        object_type_name: ObjectTypeName,
    },

    /// Indicates a type that is the same as the type of the given variable.
    Variable(TypeVariable),

    /// A type that is the same as the type of elements in the array type referenced by the
    /// variable.
    ElementOf(Box<TypeConstraint>),

    /// A type that is the same as the type of a field of an object type referenced by the
    /// variable, or that is the same as a type in a field of a field, etc.
    FieldOf {
        target_type: Box<TypeConstraint>,
        path: NonEmpty<FieldName>,
    },

    /// A type that modifies another type by adding or replacing object fields.
    WithFieldOverrides {
        target_type: Box<TypeConstraint>,
        fields: BTreeMap<FieldName, TypeConstraint>,
    },
}

impl From<ndc_models::Type> for TypeConstraint {
    fn from(t: ndc_models::Type) -> Self {
        match t {
            ndc_models::Type::Named { name } => {
                let scalar_type_name = ndc_models::ScalarTypeName::ref_cast(&name);
                match MongoScalarType::try_from(scalar_type_name) {
                    Ok(MongoScalarType::Bson(scalar_type)) => TypeConstraint::Scalar(scalar_type),
                    Ok(MongoScalarType::ExtendedJSON) => TypeConstraint::ExtendedJSON,
                    Err(_) => TypeConstraint::Object(name.into()),
                }
            }
            ndc_models::Type::Nullable { underlying_type } => {
                TypeConstraint::Nullable(Box::new(Self::from(*underlying_type)))
            }
            ndc_models::Type::Array { element_type } => {
                TypeConstraint::ArrayOf(Box::new(Self::from(*element_type)))
            }
            ndc_models::Type::Predicate { object_type_name } => {
                TypeConstraint::Predicate { object_type_name }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTypeConstraint {
    pub fields: BTreeMap<FieldName, TypeConstraint>,
}

impl From<ndc_models::ObjectType> for ObjectTypeConstraint {
    fn from(value: ndc_models::ObjectType) -> Self {
        ObjectTypeConstraint {
            fields: value
                .fields
                .into_iter()
                .map(|(name, field)| (name, field.r#type.into()))
                .collect(),
        }
    }
}
