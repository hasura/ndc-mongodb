use std::collections::BTreeMap;

use configuration::schema::{CollectionSchema, ObjectField, ObjectType, Type};
use itertools::Itertools as _;
use ndc_models::ObjectTypeName;

use super::ObjectTypes;

pub fn keep_backward_compatible_changes(
    existing_collection: CollectionSchema,
    mut updated_object_types: ObjectTypes,
) -> CollectionSchema {
    let mut accumulated_new_object_types = Default::default();
    let CollectionSchema {
        collection,
        object_types: mut previously_defined_object_types,
    } = existing_collection;
    backward_compatible_helper(
        &mut previously_defined_object_types,
        &mut updated_object_types,
        &mut accumulated_new_object_types,
        collection.r#type.clone(),
    );
    CollectionSchema {
        collection,
        object_types: accumulated_new_object_types,
    }
}

fn backward_compatible_helper(
    previously_defined_object_types: &mut ObjectTypes,
    updated_object_types: &mut ObjectTypes,
    accumulated_new_object_types: &mut ObjectTypes,
    type_name: ObjectTypeName,
) {
    if accumulated_new_object_types.contains_key(&type_name) {
        return;
    }
    let existing = previously_defined_object_types.remove(&type_name);
    let updated = updated_object_types.remove(&type_name);
    match (existing, updated) {
        (Some(existing), Some(updated)) => {
            let object_type = backward_compatible_object_type(
                previously_defined_object_types,
                updated_object_types,
                accumulated_new_object_types,
                existing,
                updated,
            );
            accumulated_new_object_types.insert(type_name, object_type);
        }
        (Some(existing), None) => {
            accumulated_new_object_types.insert(type_name, existing.clone());
        }
        (None, Some(updated)) => {
            accumulated_new_object_types.insert(type_name, updated);
        }
        // shouldn't be reachable
        (None, None) => (),
    }
}

fn backward_compatible_object_type(
    previously_defined_object_types: &mut ObjectTypes,
    updated_object_types: &mut ObjectTypes,
    accumulated_new_object_types: &mut ObjectTypes,
    existing: ObjectType,
    mut updated: ObjectType,
) -> ObjectType {
    let field_names = updated
        .fields
        .keys()
        .chain(existing.fields.keys())
        .unique()
        .cloned()
        .collect_vec();
    let fields = field_names
        .into_iter()
        .map(|name| {
            let existing_field = existing.fields.get(&name);
            let updated_field = updated.fields.remove(&name);
            let field = match (existing_field, updated_field) {
                (Some(existing_field), Some(updated_field)) => {
                    let r#type = reconcile_types(
                        previously_defined_object_types,
                        updated_object_types,
                        accumulated_new_object_types,
                        existing_field.r#type.clone(),
                        updated_field.r#type,
                    );
                    ObjectField {
                        description: existing.description.clone().or(updated_field.description),
                        r#type,
                    }
                }
                (Some(existing_field), None) => existing_field.clone(),
                (None, Some(updated_field)) => updated_field,
                (None, None) => unreachable!(),
            };
            (name.clone(), field)
        })
        .collect();
    ObjectType {
        description: existing.description.clone().or(updated.description),
        fields,
    }
}

fn reconcile_types(
    previously_defined_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    updated_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    accumulated_new_object_types: &mut BTreeMap<ObjectTypeName, ObjectType>,
    existing_type: Type,
    updated_type: Type,
) -> Type {
    match (existing_type, updated_type) {
        (Type::Nullable(a), Type::Nullable(b)) => Type::Nullable(Box::new(reconcile_types(
            previously_defined_object_types,
            updated_object_types,
            accumulated_new_object_types,
            *a,
            *b,
        ))),
        (Type::Nullable(a), b) => Type::Nullable(Box::new(reconcile_types(
            previously_defined_object_types,
            updated_object_types,
            accumulated_new_object_types,
            *a,
            b,
        ))),
        (a, Type::Nullable(b)) => reconcile_types(
            previously_defined_object_types,
            updated_object_types,
            accumulated_new_object_types,
            a,
            *b,
        ),
        (Type::ArrayOf(a), Type::ArrayOf(b)) => Type::ArrayOf(Box::new(reconcile_types(
            previously_defined_object_types,
            updated_object_types,
            accumulated_new_object_types,
            *a,
            *b,
        ))),
        (Type::Object(_), Type::Object(b)) => {
            backward_compatible_helper(
                previously_defined_object_types,
                updated_object_types,
                accumulated_new_object_types,
                b.clone().into(),
            );
            Type::Object(b)
        }
        (a, _) => a,
    }
}
