use ref_cast::RefCast;
use std::collections::BTreeMap;

use itertools::Itertools as _;
use ndc_models::{self as ndc, ArgumentName, ObjectTypeName};

use crate::{self as plan, QueryPlanError};

type Result<T> = std::result::Result<T, QueryPlanError>;

/// The type of values that a column, field, or argument may take.
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Type<ScalarType> {
    Scalar(ScalarType),
    /// The name of an object type declared in `objectTypes`
    Object(ObjectType<ScalarType>),
    ArrayOf(Box<Type<ScalarType>>),
    /// A nullable form of any of the other types
    Nullable(Box<Type<ScalarType>>),
}

impl<S> Type<S> {
    pub fn array_of(t: Self) -> Self {
        Self::ArrayOf(Box::new(t))
    }

    pub fn named_object(
        name: impl Into<ObjectTypeName>,
        fields: impl IntoIterator<Item = (impl Into<ndc::FieldName>, impl Into<ObjectField<S>>)>,
    ) -> Self {
        Self::Object(ObjectType::new(fields).named(name))
    }

    pub fn nullable(t: Self) -> Self {
        t.into_nullable()
    }

    pub fn object(
        fields: impl IntoIterator<Item = (impl Into<ndc::FieldName>, impl Into<ObjectField<S>>)>,
    ) -> Self {
        Self::Object(ObjectType::new(fields))
    }

    pub fn scalar(scalar_type: impl Into<S>) -> Self {
        Self::Scalar(scalar_type.into())
    }

    pub fn into_nullable(self) -> Self {
        match self {
            t @ Type::Nullable(_) => t,
            t => Type::Nullable(Box::new(t)),
        }
    }

    pub fn is_array(&self) -> bool {
        match self {
            Type::ArrayOf(_) => true,
            Type::Nullable(t) => t.is_array(),
            _ => false,
        }
    }

    pub fn into_array_element_type(self) -> Result<Self>
    where
        S: Clone + std::fmt::Debug,
    {
        match self {
            Type::ArrayOf(t) => Ok(*t),
            Type::Nullable(t) => t.into_array_element_type(),
            t => Err(QueryPlanError::TypeMismatch(format!(
                "expected an array, but got type {t:?}"
            ))),
        }
    }

    pub fn into_object_type(self) -> Result<ObjectType<S>>
    where
        S: std::fmt::Debug,
    {
        match self {
            Type::Object(object_type) => Ok(object_type),
            Type::Nullable(t) => t.into_object_type(),
            t => Err(QueryPlanError::TypeMismatch(format!(
                "expected object type, but got {t:?}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct ObjectType<ScalarType> {
    /// A type name may be tracked for error reporting. The name does not affect how query plans
    /// are generated.
    pub name: Option<ndc::ObjectTypeName>,
    pub fields: BTreeMap<ndc::FieldName, ObjectField<ScalarType>>,
}

impl<S> ObjectType<S> {
    pub fn new(
        fields: impl IntoIterator<Item = (impl Into<ndc::FieldName>, impl Into<ObjectField<S>>)>,
    ) -> Self {
        ObjectType {
            name: None,
            fields: fields
                .into_iter()
                .map(|(name, field)| (name.into(), field.into()))
                .collect(),
        }
    }

    pub fn named(mut self, name: impl Into<ndc::ObjectTypeName>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn named_fields(&self) -> impl Iterator<Item = (&ndc::FieldName, &Type<S>)> {
        self.fields
            .iter()
            .map(|(name, field)| (name, &field.r#type))
    }

    pub fn get(&self, field_name: &ndc::FieldName) -> Result<&ObjectField<S>> {
        self.fields
            .get(field_name)
            .ok_or_else(|| QueryPlanError::UnknownObjectTypeField {
                object_type: None,
                field_name: field_name.clone(),
                path: Default::default(),
            })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ObjectField<ScalarType> {
    pub r#type: Type<ScalarType>,
    /// The arguments available to the field - Matches implementation from CollectionInfo
    pub parameters: BTreeMap<ArgumentName, Type<ScalarType>>,
}

impl<S> ObjectField<S> {
    pub fn new(r#type: Type<S>) -> Self {
        Self {
            r#type,
            parameters: Default::default(),
        }
    }

    pub fn into_nullable(self) -> Self {
        let new_field_type = match self.r#type {
            t @ Type::Nullable(_) => t,
            t => Type::Nullable(Box::new(t)),
        };
        Self {
            r#type: new_field_type,
            parameters: self.parameters,
        }
    }

    pub fn with_parameters(mut self, parameters: BTreeMap<ArgumentName, Type<S>>) -> Self {
        self.parameters = parameters;
        self
    }
}

impl<S> From<Type<S>> for ObjectField<S> {
    fn from(value: Type<S>) -> Self {
        ObjectField {
            r#type: value,
            parameters: Default::default(),
        }
    }
}

/// Convert from ndc IR types to query plan types. The key differences are:
/// - query plan types use inline copies of object types instead of referencing object types by name
/// - query plan types are parameterized over the specific scalar type for a connector instead of
///   referencing scalar types by name
pub fn inline_object_types<ScalarType>(
    object_types: &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
    t: &ndc::Type,
    lookup_scalar_type: fn(&ndc::ScalarTypeName) -> Option<ScalarType>,
) -> Result<Type<ScalarType>> {
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
            ndc::Type::Predicate { .. } => Err(QueryPlanError::UnexpectedPredicate)?,
        };
    Ok(plan_type)
}

fn lookup_type<ScalarType>(
    object_types: &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
    name: &ndc::TypeName,
    lookup_scalar_type: fn(&ndc::ScalarTypeName) -> Option<ScalarType>,
) -> Result<plan::Type<ScalarType>> {
    if let Some(scalar_type) = lookup_scalar_type(ndc::ScalarTypeName::ref_cast(name)) {
        return Ok(Type::Scalar(scalar_type));
    }
    let object_type = lookup_object_type_helper(
        object_types,
        ndc::ObjectTypeName::ref_cast(name),
        lookup_scalar_type,
    )?;
    Ok(Type::Object(object_type))
}

fn lookup_object_type_helper<ScalarType>(
    object_types: &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
    name: &ndc::ObjectTypeName,
    lookup_scalar_type: fn(&ndc::ScalarTypeName) -> Option<ScalarType>,
) -> Result<plan::ObjectType<ScalarType>> {
    let object_type = object_types
        .get(name)
        .ok_or_else(|| QueryPlanError::UnknownObjectType(name.to_string()))?;

    let plan_object_type = plan::ObjectType {
        name: Some(name.clone()),
        fields: object_type
            .fields
            .iter()
            .map(|(name, field)| {
                let field_type =
                    inline_object_types(object_types, &field.r#type, lookup_scalar_type)?;
                Ok((
                    name.to_owned(),
                    plan::ObjectField {
                        r#type: field_type,
                        parameters: Default::default(), // TODO: connect ndc arguments to plan
                                                        // parameters
                    },
                ))
            })
            .try_collect::<_, _, QueryPlanError>()?,
    };
    Ok(plan_object_type)
}

pub fn lookup_object_type<ScalarType>(
    object_types: &BTreeMap<ndc::ObjectTypeName, ndc::ObjectType>,
    name: &ndc::ObjectTypeName,
    lookup_scalar_type: fn(&ndc::ScalarTypeName) -> Option<ScalarType>,
) -> Result<plan::ObjectType<ScalarType>> {
    lookup_object_type_helper(object_types, name, lookup_scalar_type)
}
