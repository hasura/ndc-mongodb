use std::collections::BTreeMap;

use configuration::{
    schema::{ObjectField, ObjectType, Type},
    Configuration,
};
use ndc_models::{FieldName, ObjectTypeName};

use crate::native_query::{
    error::{Error, Result},
    type_constraint::{ObjectTypeConstraint, TypeConstraint},
};

use TypeConstraint as C;

/// In cases where there is enough information present in one constraint itself to infer a concrete
/// type, do that. Returns None if there is not enough information present.
// pub fn constraint_to_type(
//     configuration: &Configuration,
//     object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
//     constraint: &TypeConstraint,
// ) -> Result<Option<(Type, BTreeMap<ObjectTypeName, ObjectType>)>> {
//     let mut solved_object_types = BTreeMap::new();
//     let solved_type = constraint_to_type_helper(
//         configuration,
//         &mut solved_object_types,
//         object_type_constraints,
//         constraint,
//     )?;
//     Ok(solved_type.map(|t| (t, solved_object_types)))
// }

pub fn constraint_to_type(
    configuration: &Configuration,
    added_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    constraint: &TypeConstraint,
) -> Result<Option<Type>> {
    let solution = match constraint {
        C::ExtendedJSON => Some(Type::ExtendedJSON),
        C::Scalar(s) => Some(Type::Scalar(s.clone())),
        C::ArrayOf(c) => constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            c,
        )?
        .map(|t| Type::ArrayOf(Box::new(t))),
        C::Object(name) => object_constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            name,
        )?
        .map(|_| Type::Object(name.to_string())),
        C::Predicate { object_type_name } => object_constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            object_type_name,
        )?
        .map(|_| Type::Predicate {
            object_type_name: object_type_name.clone(),
        }),
        C::Nullable(c) => constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            c,
        )?
        .map(|t| Type::Nullable(Box::new(t))),
        C::Variable(_) => None,
        C::ElementOf(c) => constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            c,
        )?
        .map(|t| element_of(t))
        .transpose()?,
        C::FieldOf { target_type, path } => constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            target_type,
        )?
        .and_then(|t| {
            field_of(
                configuration,
                added_object_types,
                object_type_constraints,
                t,
                path,
            )
            .transpose()
        })
        .transpose()?,
        C::WithFieldOverrides {
            augmented_object_type_name,
            target_type,
            fields,
        } => {
            let resolved_object_type = constraint_to_type(
                configuration,
                added_object_types,
                object_type_constraints,
                target_type,
            )?;
            let resolved_field_types: Option<Vec<(FieldName, Type)>> = fields
                .iter()
                .map(|(field_name, t)| {
                    Ok(constraint_to_type(
                        configuration,
                        added_object_types,
                        object_type_constraints,
                        t,
                    )?
                    .map(|t| (field_name.clone(), t)))
                })
                .collect::<Result<_>>()?;
            match (resolved_object_type, resolved_field_types) {
                (Some(object_type), Some(fields)) => with_field_overrides(
                    configuration,
                    added_object_types,
                    object_type_constraints,
                    object_type,
                    augmented_object_type_name.clone(),
                    fields,
                )?,
                _ => None,
            }
        }
    };
    Ok(solution)
}

fn object_constraint_to_type(
    configuration: &Configuration,
    added_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    name: &ObjectTypeName,
) -> Result<Option<ObjectType>> {
    // If the referenced type is defined externally to the native query or already has a recorded
    // solution then we don't need to do anything.
    if let Some(object_type) = configuration.object_types.get(name) {
        return Ok(Some(object_type.clone().into()));
    }
    if let Some(object_type) = added_object_types.get(name) {
        return Ok(Some(object_type.clone()));
    }

    let Some(object_type_constraint) = object_type_constraints.get(name).cloned() else {
        return Err(Error::UnknownObjectType(name.to_string()));
    };

    let mut fields = BTreeMap::new();
    // let mut solved_object_types = BTreeMap::new();

    for (field_name, field_constraint) in object_type_constraint.fields.iter() {
        match constraint_to_type(
            configuration,
            added_object_types,
            object_type_constraints,
            field_constraint,
        )? {
            Some(solved_field_type) => {
                fields.insert(
                    field_name.clone(),
                    ObjectField {
                        r#type: solved_field_type,
                        description: None,
                    },
                );
            }
            // If any fields do not have solved types we need to abort
            None => return Ok(None),
        };
    }

    let new_object_type = ObjectType {
        fields,
        description: None,
    };
    added_object_types.insert(name.clone(), new_object_type.clone());

    Ok(Some(new_object_type))
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
    configuration: &Configuration,
    added_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    object_type: Type,
    path: impl IntoIterator<Item = &'a FieldName>,
) -> Result<Option<Type>> {
    let field_type = match object_type {
        Type::ExtendedJSON => Ok(Some(Type::ExtendedJSON)),
        Type::Object(type_name) => {
            let Some(object_type) = object_constraint_to_type(
                configuration,
                added_object_types,
                object_type_constraints,
                &type_name.clone().into(),
            )?
            else {
                return Ok(None);
            };

            let mut path_iter = path.into_iter();
            let Some(field_name) = path_iter.next() else {
                return Ok(Some(Type::Object(type_name)));
            };

            let field_type =
                object_type
                    .fields
                    .get(field_name)
                    .ok_or(Error::ObjectMissingField {
                        object_type: type_name.into(),
                        field_name: field_name.clone(),
                    })?;

            Ok(Some(field_type.r#type.clone()))
        }
        Type::Nullable(t) => {
            let underlying_type = field_of(
                configuration,
                added_object_types,
                object_type_constraints,
                *t,
                path,
            )?;
            Ok(underlying_type.map(|t| Type::Nullable(Box::new(t))))
        }
        t => Err(Error::ExpectedObject { actual_type: t }),
    }?;
    Ok(field_type.map(Type::normalize_type))
}

fn with_field_overrides(
    configuration: &Configuration,
    added_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    object_type_constraints: &mut BTreeMap<ObjectTypeName, ObjectTypeConstraint>,
    object_type: Type,
    augmented_object_type_name: ObjectTypeName,
    fields: impl IntoIterator<Item = (FieldName, Type)>,
) -> Result<Option<Type>> {
    let augmented_object_type = match object_type {
        Type::ExtendedJSON => Some(Type::ExtendedJSON),
        Type::Object(type_name) => {
            let Some(object_type) = object_constraint_to_type(
                configuration,
                added_object_types,
                object_type_constraints,
                &type_name.clone().into(),
            )?
            else {
                return Ok(None);
            };
            let mut new_object_type = object_type.clone();
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
            added_object_types.insert(augmented_object_type_name.clone(), new_object_type);
            Some(Type::Object(augmented_object_type_name.to_string()))
        }
        Type::Nullable(t) => {
            let underlying_type = with_field_overrides(
                configuration,
                added_object_types,
                object_type_constraints,
                *t,
                augmented_object_type_name,
                fields,
            )?;
            underlying_type.map(|t| Type::Nullable(Box::new(t)))
        }
        t => Err(Error::ExpectedObject { actual_type: t })?,
    };
    Ok(augmented_object_type.map(Type::normalize_type))
}
