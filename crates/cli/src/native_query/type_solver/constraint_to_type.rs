use std::collections::BTreeMap;

use configuration::schema::{ObjectField, ObjectType, Type};
use ndc_models::{FieldName, ObjectTypeName};

use crate::native_query::{
    error::{Error, Result},
    type_constraint::TypeConstraint,
};

use TypeConstraint as C;

/// In cases where there is enough information present in the constraint itself to infer a concrete
/// type, do that. Returns None if there is not enough information present.
pub fn constraint_to_type(
    object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    constraint: &TypeConstraint,
) -> Result<Option<Type>> {
    let solution = match constraint {
        C::ExtendedJSON => Some(Type::ExtendedJSON),
        C::Scalar(s) => Some(Type::Scalar(s.clone())),
        C::Object(name) => Some(Type::Object(name.to_string())),
        C::ArrayOf(c) => constraint_to_type(object_types, c)?.map(|t| Type::ArrayOf(Box::new(t))),
        C::Nullable(c) => constraint_to_type(object_types, c)?.map(|t| Type::Nullable(Box::new(t))),
        C::Predicate { object_type_name } => Some(Type::Predicate {
            object_type_name: object_type_name.clone(),
        }),
        C::Variable(_) => None,
        C::ElementOf(c) => constraint_to_type(object_types, c)?
            .map(element_of)
            .transpose()?,
        C::FieldOf { target_type, path } => constraint_to_type(object_types, target_type)?
            .map(|t| field_of(object_types, t, path))
            .transpose()?,
        C::WithFieldOverrides {
            augmented_object_type_name,
            target_type,
            fields,
        } => {
            let resolved_object_type = constraint_to_type(object_types, target_type)?;
            let resolved_field_types: Option<Vec<(FieldName, Type)>> = fields
                .iter()
                .map(|(field_name, t)| {
                    Ok(constraint_to_type(object_types, t)?.map(|t| (field_name.clone(), t)))
                })
                .collect::<Result<_>>()?;
            match (resolved_object_type, resolved_field_types) {
                (Some(object_type), Some(fields)) => Some(with_field_overrides(
                    object_types,
                    object_type,
                    augmented_object_type_name.clone(),
                    fields,
                )?),
                _ => None,
            }
        }
    };
    Ok(solution)
}

fn element_of(array_type: Type) -> Result<Type> {
    let element_type = match array_type {
        Type::ArrayOf(elem_type) => Ok(*elem_type),
        Type::Nullable(t) => element_of(*t).map(|t| Type::Nullable(Box::new(t))),
        _ => Err(Error::ExpectedArray {
            actual_type: array_type,
        }),
    }?;
    Ok(element_type.normalize_type())
}

fn field_of<'a>(
    object_types: &BTreeMap<ObjectTypeName, ObjectType>,
    object_type: Type,
    path: impl IntoIterator<Item = &'a FieldName>,
) -> Result<Type> {
    let field_type = match object_type {
        Type::ExtendedJSON => Ok(Type::ExtendedJSON),
        Type::Object(type_name) => {
            let mut path_iter = path.into_iter();
            let Some(field_name) = path_iter.next() else {
                return Ok(Type::Object(type_name));
            };

            let object_type = object_types
                .get::<ObjectTypeName>(&(type_name.clone().into()))
                .ok_or_else(|| Error::UnknownObjectType(type_name.clone()))?;

            let field_type =
                object_type
                    .fields
                    .get(field_name)
                    .ok_or(Error::ObjectMissingField {
                        object_type: type_name.into(),
                        field_name: field_name.clone(),
                    })?;

            Ok(field_type.r#type.clone())
        }
        Type::Nullable(t) => {
            let underlying_type = field_of(object_types, *t, path)?;
            Ok(Type::Nullable(Box::new(underlying_type)))
        }
        t => Err(Error::ExpectedObject { actual_type: t }),
    }?;
    Ok(field_type.normalize_type())
}

fn with_field_overrides(
    object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    object_type: Type,
    augmented_object_type_name: ObjectTypeName,
    fields: impl IntoIterator<Item = (FieldName, Type)>,
) -> Result<Type> {
    let augmented_object_type = match object_type {
        Type::ExtendedJSON => Ok(Type::ExtendedJSON),
        Type::Object(type_name) => {
            let mut new_object_type = object_types
                .get::<ObjectTypeName>(&(type_name.clone().into()))
                .ok_or_else(|| Error::UnknownObjectType(type_name.clone()))?
                .clone();
            for (field_name, field_type) in fields.into_iter() {
                new_object_type.fields.insert(
                    field_name,
                    ObjectField {
                        r#type: field_type,
                        description: None,
                    },
                );
            }
            // TODO: We might end up back-tracking in which case this will register an object type
            // that isn't referenced. BUT once solving is complete we should get here again with
            // the same augmented_object_type_name, overwrite the old definition with an identical
            // one, and then it will be referenced.
            object_types.insert(augmented_object_type_name.clone(), new_object_type);
            Ok(Type::Object(augmented_object_type_name.to_string()))
        }
        Type::Nullable(t) => {
            let underlying_type =
                with_field_overrides(object_types, *t, augmented_object_type_name, fields)?;
            Ok(Type::Nullable(Box::new(underlying_type)))
        }
        t => Err(Error::ExpectedObject { actual_type: t }),
    }?;
    Ok(augmented_object_type.normalize_type())
}
