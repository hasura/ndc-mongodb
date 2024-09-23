use std::collections::BTreeMap;

use deriving_via::DerivingVia;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};

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
    ExtendedJSON,
    Scalar(BsonScalarType),
    Object(ObjectTypeName),
    ArrayOf(Box<TypeConstraint>),
    Nullable(Box<TypeConstraint>),
    Predicate { object_type_name: ObjectTypeName },
    Variable(TypeVariable),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectTypeConstraint {
    pub fields: BTreeMap<FieldName, TypeConstraint>,
}
