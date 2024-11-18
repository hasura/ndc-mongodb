use std::collections::{BTreeMap, HashSet};

use configuration::schema::{ObjectField, ObjectType, Type};
use itertools::Itertools as _;
use ndc_models::ObjectTypeName;

use crate::native_query::helpers::{parse_counter_suffix, unique_type_name};

use super::error::{Error, Result};

/// Filters map of object types to get only types that are referenced directly or indirectly from
/// the set of reference types.
pub fn prune_object_types(
    reference_types: &mut [&mut Type],
    existing_object_types: &BTreeMap<ObjectTypeName, ndc_models::ObjectType>,
    added_object_types: BTreeMap<ObjectTypeName, ObjectType>,
) -> Result<BTreeMap<ObjectTypeName, ObjectType>> {
    let mut required_type_names = HashSet::new();
    for t in &*reference_types {
        collect_names_from_type(
            existing_object_types,
            &added_object_types,
            &mut required_type_names,
            t,
        )?;
    }
    let mut pruned_object_types = added_object_types
        .into_iter()
        .filter(|(name, _)| required_type_names.contains(name))
        .collect();

    simplify_type_names(
        reference_types,
        existing_object_types,
        &mut pruned_object_types,
    );

    Ok(pruned_object_types)
}

fn collect_names_from_type(
    existing_object_types: &BTreeMap<ObjectTypeName, ndc_models::ObjectType>,
    added_object_types: &BTreeMap<ObjectTypeName, ObjectType>,
    found_type_names: &mut HashSet<ObjectTypeName>,
    input_type: &Type,
) -> Result<()> {
    match input_type {
        Type::Object(type_name) => {
            let object_type_name = mk_object_type_name(type_name);
            collect_names_from_object_type(
                existing_object_types,
                added_object_types,
                found_type_names,
                &object_type_name,
            )?;
            found_type_names.insert(object_type_name);
        }
        Type::Predicate { object_type_name } => {
            let object_type_name = object_type_name.clone();
            collect_names_from_object_type(
                existing_object_types,
                added_object_types,
                found_type_names,
                &object_type_name,
            )?;
            found_type_names.insert(object_type_name);
        }
        Type::ArrayOf(t) => collect_names_from_type(
            existing_object_types,
            added_object_types,
            found_type_names,
            t,
        )?,
        Type::Nullable(t) => collect_names_from_type(
            existing_object_types,
            added_object_types,
            found_type_names,
            t,
        )?,
        Type::ExtendedJSON => (),
        Type::Scalar(_) => (),
    };
    Ok(())
}

fn collect_names_from_object_type(
    existing_object_types: &BTreeMap<ObjectTypeName, ndc_models::ObjectType>,
    object_types: &BTreeMap<ObjectTypeName, ObjectType>,
    found_type_names: &mut HashSet<ObjectTypeName>,
    input_type_name: &ObjectTypeName,
) -> Result<()> {
    if existing_object_types.contains_key(input_type_name) {
        return Ok(());
    }
    let object_type = object_types
        .get(input_type_name)
        .ok_or_else(|| Error::UnknownObjectType(input_type_name.to_string()))?;
    for (_, field) in object_type.fields.iter() {
        collect_names_from_type(
            existing_object_types,
            object_types,
            found_type_names,
            &field.r#type,
        )?;
    }
    Ok(())
}

/// The system for generating unique object type names uses numeric suffixes. After pruning we may
/// be able to remove these suffixes.
fn simplify_type_names(
    reference_types: &mut [&mut Type],
    existing_object_types: &BTreeMap<ObjectTypeName, ndc_models::ObjectType>,
    added_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
) {
    let names = added_object_types.keys().cloned().collect_vec();
    for name in names {
        let (name_root, count) = parse_counter_suffix(name.as_str());
        let maybe_simplified_name =
            unique_type_name(existing_object_types, added_object_types, &name_root);
        let (_, new_count) = parse_counter_suffix(maybe_simplified_name.as_str());
        if new_count < count {
            rename_object_type(
                reference_types,
                added_object_types,
                &name,
                &maybe_simplified_name,
            );
        }
    }
}

fn rename_object_type(
    reference_types: &mut [&mut Type],
    object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    old_name: &ObjectTypeName,
    new_name: &ObjectTypeName,
) {
    for t in reference_types.iter_mut() {
        **t = rename_type_helper(old_name, new_name, (*t).clone());
    }

    let renamed_object_types = object_types
        .clone()
        .into_iter()
        .map(|(name, object_type)| {
            let new_type_name = if &name == old_name {
                new_name.clone()
            } else {
                name
            };
            let new_object_type = rename_object_type_helper(old_name, new_name, object_type);
            (new_type_name, new_object_type)
        })
        .collect();
    *object_types = renamed_object_types;
}

fn rename_type_helper(
    old_name: &ObjectTypeName,
    new_name: &ObjectTypeName,
    input_type: Type,
) -> Type {
    let old_name_string = old_name.to_string();

    match input_type {
        Type::Object(name) => {
            if name == old_name_string {
                Type::Object(new_name.to_string())
            } else {
                Type::Object(name)
            }
        }
        Type::Predicate { object_type_name } => {
            if &object_type_name == old_name {
                Type::Predicate {
                    object_type_name: new_name.clone(),
                }
            } else {
                Type::Predicate { object_type_name }
            }
        }
        Type::ArrayOf(t) => Type::ArrayOf(Box::new(rename_type_helper(old_name, new_name, *t))),
        Type::Nullable(t) => Type::Nullable(Box::new(rename_type_helper(old_name, new_name, *t))),
        t @ Type::Scalar(_) => t,
        t @ Type::ExtendedJSON => t,
    }
}

fn rename_object_type_helper(
    old_name: &ObjectTypeName,
    new_name: &ObjectTypeName,
    object_type: ObjectType,
) -> ObjectType {
    let new_fields = object_type
        .fields
        .into_iter()
        .map(|(name, field)| {
            let new_field = ObjectField {
                r#type: rename_type_helper(old_name, new_name, field.r#type),
                description: field.description,
            };
            (name, new_field)
        })
        .collect();
    ObjectType {
        fields: new_fields,
        description: object_type.description,
    }
}

fn mk_object_type_name(name: &str) -> ObjectTypeName {
    name.into()
}

#[cfg(test)]
mod tests {
    use configuration::schema::{ObjectField, ObjectType, Type};
    use googletest::prelude::*;

    use super::prune_object_types;

    #[googletest::test]
    fn prunes_and_simplifies_object_types() -> Result<()> {
        let mut result_type = Type::Object("Documents_2".into());
        let mut reference_types = [&mut result_type];
        let existing_object_types = Default::default();

        let added_object_types = [
            (
                "Documents_1".into(),
                ObjectType {
                    fields: [(
                        "bar".into(),
                        ObjectField {
                            r#type: Type::Scalar(mongodb_support::BsonScalarType::String),
                            description: None,
                        },
                    )]
                    .into(),
                    description: None,
                },
            ),
            (
                "Documents_2".into(),
                ObjectType {
                    fields: [(
                        "foo".into(),
                        ObjectField {
                            r#type: Type::Scalar(mongodb_support::BsonScalarType::String),
                            description: None,
                        },
                    )]
                    .into(),
                    description: None,
                },
            ),
        ]
        .into();

        let pruned = prune_object_types(
            &mut reference_types,
            &existing_object_types,
            added_object_types,
        )?;

        expect_eq!(
            pruned,
            [(
                "Documents".into(),
                ObjectType {
                    fields: [(
                        "foo".into(),
                        ObjectField {
                            r#type: Type::Scalar(mongodb_support::BsonScalarType::String),
                            description: None,
                        },
                    )]
                    .into(),
                    description: None,
                },
            )]
            .into()
        );

        expect_eq!(result_type, Type::Object("Documents".into()));

        Ok(())
    }
}
