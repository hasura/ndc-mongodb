use std::collections::{BTreeMap, BTreeSet};

use configuration::MongoScalarType;
use mongodb_support::BsonScalarType;
use ndc_models::{FieldName, ObjectTypeName};
use nonempty::NonEmpty;
use ref_cast::RefCast as _;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TypeVariable {
    id: u32,
    pub variance: Variance,
}

impl TypeVariable {
    pub fn new(id: u32, variance: Variance) -> Self {
        TypeVariable { id, variance }
    }

    pub fn is_covariant(self) -> bool {
        matches!(self.variance, Variance::Covariant)
    }

    pub fn is_contravariant(self) -> bool {
        matches!(self.variance, Variance::Contravariant)
    }
}

impl std::fmt::Display for TypeVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "${}", self.id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Variance {
    Covariant,
    Contravariant,
    Invariant,
}

/// A TypeConstraint is almost identical to a [configuration::schema::Type], except that
/// a TypeConstraint may reference type variables.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum TypeConstraint {
    // Normal type stuff - except that composite types might include variables in their structure.
    ExtendedJSON,
    Scalar(BsonScalarType),
    Object(ObjectTypeName),
    ArrayOf(Box<TypeConstraint>),
    Predicate {
        object_type_name: ObjectTypeName,
    },

    // Complex types
    
    Union(BTreeSet<TypeConstraint>),

    /// Unlike Union we expect the solved concrete type for a variable with a OneOf constraint may
    /// be one of the types in the set, but we don't know yet which one. This is useful for MongoDB
    /// operators that expect an input of any numeric type. We use OneOf because we don't know
    /// which numeric type to infer until we see more usage evidence of the same type variable.
    ///
    /// In other words with Union we have specific evidence that a variable occurs in contexts of
    /// multiple concrete types, while with OneOf we **don't** have specific evidence that the
    /// variable takes multiple types, but there are multiple possibilities of the type or types
    /// that it does take.
    OneOf(BTreeSet<TypeConstraint>),

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

    /// A type that modifies another type by adding, replacing, or subtracting object fields.
    WithFieldOverrides {
        augmented_object_type_name: ObjectTypeName,
        target_type: Box<TypeConstraint>,
        fields: BTreeMap<FieldName, Option<TypeConstraint>>,
    },
}

impl TypeConstraint {
    /// Order constraints by complexity to help with type unification
    pub fn complexity(&self) -> usize {
        match self {
            TypeConstraint::Variable(_) => 2,
            TypeConstraint::ExtendedJSON => 0,
            TypeConstraint::Scalar(_) => 0,
            TypeConstraint::Object(_) => 1,
            TypeConstraint::Predicate { .. } => 1,
            TypeConstraint::ArrayOf(constraint) => 1 + constraint.complexity(),
            TypeConstraint::Union(constraints) => {
                1 + constraints
                    .iter()
                    .map(TypeConstraint::complexity)
                    .sum::<usize>()
            }
            TypeConstraint::OneOf(constraints) => {
                1 + constraints
                    .iter()
                    .map(TypeConstraint::complexity)
                    .sum::<usize>()
            }
            TypeConstraint::ElementOf(constraint) => 2 + constraint.complexity(),
            TypeConstraint::FieldOf { target_type, path } => {
                2 + target_type.complexity() + path.len()
            }
            TypeConstraint::WithFieldOverrides {
                target_type,
                fields,
                ..
            } => {
                let overridden_field_complexity: usize = fields
                    .values()
                    .flatten()
                    .map(|constraint| constraint.complexity())
                    .sum();
                2 + target_type.complexity() + overridden_field_complexity
            }
        }
    }

    pub fn make_nullable(self) -> Self {
        match self {
            TypeConstraint::ExtendedJSON => TypeConstraint::ExtendedJSON,
            t @ TypeConstraint::Scalar(BsonScalarType::Null) => t,
            t => TypeConstraint::union(t, TypeConstraint::Scalar(BsonScalarType::Null)),
        }
    }

    pub fn null() -> Self {
        TypeConstraint::Scalar(BsonScalarType::Null)
    }

    pub fn is_nullable(&self) -> bool {
        match self {
            TypeConstraint::Union(types) => types
                .iter()
                .any(|t| matches!(t, TypeConstraint::Scalar(BsonScalarType::Null))),
            _ => false,
        }
    }

    pub fn map_nullable<F>(self, callback: F) -> TypeConstraint
    where
        F: FnOnce(TypeConstraint) -> TypeConstraint,
    {
        match self {
            Self::Union(types) => {
                let non_null_types: BTreeSet<_> =
                    types.into_iter().filter(|t| t != &Self::null()).collect();
                let single_non_null_type = if non_null_types.len() == 1 {
                    non_null_types.into_iter().next().unwrap()
                } else {
                    Self::Union(non_null_types)
                };
                let mapped = callback(single_non_null_type);
                Self::union(mapped, Self::null())
            }
            t => callback(t),
        }
    }

    fn scalar_one_of_by_predicate(f: impl Fn(BsonScalarType) -> bool) -> TypeConstraint {
        let matching_types = enum_iterator::all::<BsonScalarType>()
            .filter(|t| f(*t))
            .map(TypeConstraint::Scalar)
            .collect();
        TypeConstraint::OneOf(matching_types)
    }

    pub fn comparable() -> TypeConstraint {
        Self::scalar_one_of_by_predicate(BsonScalarType::is_comparable)
    }

    pub fn numeric() -> TypeConstraint {
        Self::scalar_one_of_by_predicate(BsonScalarType::is_numeric)
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            TypeConstraint::Scalar(scalar_type) => BsonScalarType::is_numeric(*scalar_type),
            TypeConstraint::OneOf(types) => types.iter().all(|t| t.is_numeric()),
            TypeConstraint::Union(types) => types.iter().all(|t| t.is_numeric()),
            _ => false,
        }
    }

    pub fn union(a: TypeConstraint, b: TypeConstraint) -> Self {
        match (a, b) {
            (TypeConstraint::Union(mut types_a), TypeConstraint::Union(mut types_b)) => {
                types_a.append(&mut types_b);
                TypeConstraint::Union(types_a)
            }
            (TypeConstraint::Union(mut types), b) => {
                types.insert(b);
                TypeConstraint::Union(types)
            }
            (a, TypeConstraint::Union(mut types)) => {
                types.insert(a);
                TypeConstraint::Union(types)
            }
            (a, b) => TypeConstraint::Union([a, b].into()),
        }
    }
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
                Self::from(*underlying_type).make_nullable()
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

impl From<configuration::schema::Type> for TypeConstraint {
    fn from(t: configuration::schema::Type) -> Self {
        match t {
            configuration::schema::Type::ExtendedJSON => TypeConstraint::ExtendedJSON,
            configuration::schema::Type::Scalar(s) => TypeConstraint::Scalar(s),
            configuration::schema::Type::Object(name) => TypeConstraint::Object(name.into()),
            configuration::schema::Type::ArrayOf(t) => {
                TypeConstraint::ArrayOf(Box::new(TypeConstraint::from(*t)))
            }
            configuration::schema::Type::Nullable(t) => TypeConstraint::from(*t).make_nullable(),
            configuration::schema::Type::Predicate { object_type_name } => {
                TypeConstraint::Predicate { object_type_name }
            }
        }
    }
}

impl From<&configuration::schema::Type> for TypeConstraint {
    fn from(t: &configuration::schema::Type) -> Self {
        t.clone().into()
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
