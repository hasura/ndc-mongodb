use std::collections::BTreeMap;

use itertools::Itertools as _;
use ndc_models as ndc;

use crate::{self as plan, QueryPlanError};

/// The type of values that a column, field, or argument may take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Type<ScalarType> {
    Scalar(ScalarType),
    /// The name of an object type declared in `objectTypes`
    Object(ObjectType<ScalarType>),
    ArrayOf(Box<Type<ScalarType>>),
    /// A nullable form of any of the other types
    Nullable(Box<Type<ScalarType>>),
}

impl<S> Type<S> {
    pub fn into_nullable(self) -> Self {
        match self {
            t @ Type::Nullable(_) => t,
            t => Type::Nullable(Box::new(t)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectType<ScalarType> {
    /// A type name may be tracked for error reporting. The name does not affect how query plans
    /// are generated.
    pub name: Option<String>,
    pub fields: BTreeMap<String, Type<ScalarType>>,
}

impl<S> ObjectType<S> {
    pub fn named_fields(&self) -> impl Iterator<Item = (&str, &Type<S>)> {
        self.fields
            .iter()
            .map(|(name, field)| (name.as_ref(), field))
    }
}

/// Convert from ndc IR types to query plan types. The key differences are:
/// - query plan types use inline copies of object types instead of referencing object types by name
/// - query plan types are parameterized over the specific scalar type for a connector instead of
///   referencing scalar types by name
pub fn inline_object_types<ScalarType>(
    object_types: &BTreeMap<String, ndc::ObjectType>,
    t: &ndc::Type,
    lookup_scalar_type: fn(&str) -> Option<ScalarType>,
) -> Result<Type<ScalarType>, QueryPlanError> {
    let plan_type =
        match t {
            ndc::Type::Named { name } => lookup_type(object_types, name, lookup_scalar_type)?,
            ndc::Type::Nullable { underlying_type } => Type::Nullable(Box::new(
                inline_object_types(object_types, underlying_type, lookup_scalar_type)?,
            )),
            ndc::Type::Array { element_type } => Type::ArrayOf(Box::new(inline_object_types(
                object_types,
                element_type,
                lookup_scalar_type,
            )?)),
            ndc::Type::Predicate { .. } => Err(QueryPlanError::NotImplemented("predicate types"))?,
        };
    Ok(plan_type)
}

fn lookup_type<ScalarType>(
    object_types: &BTreeMap<String, ndc::ObjectType>,
    name: &str,
    lookup_scalar_type: fn(&str) -> Option<ScalarType>,
) -> Result<plan::Type<ScalarType>, QueryPlanError> {
    if let Some(scalar_type) = lookup_scalar_type(name) {
        return Ok(Type::Scalar(scalar_type));
    }
    let object_type = lookup_object_type_helper(object_types, name, lookup_scalar_type)?;
    Ok(Type::Object(object_type))
}

fn lookup_object_type_helper<ScalarType>(
    object_types: &BTreeMap<String, ndc::ObjectType>,
    name: &str,
    lookup_scalar_type: fn(&str) -> Option<ScalarType>,
) -> Result<plan::ObjectType<ScalarType>, QueryPlanError> {
    let object_type = object_types
        .get(name)
        .ok_or_else(|| QueryPlanError::UnknownObjectType(name.to_string()))?;

    let plan_object_type = plan::ObjectType {
        name: Some(name.to_owned()),
        fields: object_type
            .fields
            .iter()
            .map(|(name, field)| {
                Ok((
                    name.to_owned(),
                    inline_object_types(object_types, &field.r#type, lookup_scalar_type)?,
                ))
            })
            .try_collect()?,
    };
    Ok(plan_object_type)
}

pub fn lookup_object_type<ScalarType>(
    object_types: &BTreeMap<String, ndc::ObjectType>,
    name: &str,
    lookup_scalar_type: fn(&str) -> Option<ScalarType>,
) -> Result<plan::ObjectType<ScalarType>, QueryPlanError> {
    lookup_object_type_helper(object_types, name, lookup_scalar_type)
}
