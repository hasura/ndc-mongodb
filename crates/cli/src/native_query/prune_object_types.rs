use std::collections::{BTreeMap, HashSet};

use configuration::schema::{ObjectType, Type};
use ndc_models::ObjectTypeName;

use super::error::{Error, Result};

/// Filters map of object types to get only types that are referenced directly or indirectly from
/// the set of reference types.
pub fn prune_object_types<'a>(
    reference_types: impl IntoIterator<Item = &'a Type>,
    object_types: BTreeMap<ObjectTypeName, ObjectType>,
) -> Result<BTreeMap<ObjectTypeName, ObjectType>> {
    let mut required_type_names = HashSet::new();
    for t in reference_types {
        collect_names_from_type(&object_types, &mut required_type_names, t)?;
    }
    let pruned_object_types = object_types
        .into_iter()
        .filter(|(name, _)| required_type_names.contains(name))
        .collect();
    Ok(pruned_object_types)
}

fn collect_names_from_type(
    object_types: &BTreeMap<ObjectTypeName, ObjectType>,
    found_type_names: &mut HashSet<ObjectTypeName>,
    input_type: &Type,
) -> Result<()> {
    match input_type {
        Type::Object(type_name) => {
            let object_type_name = mk_object_type_name(type_name);
            collect_names_from_object_type(object_types, found_type_names, &object_type_name)?;
            found_type_names.insert(object_type_name);
        }
        Type::Predicate { object_type_name } => {
            let object_type_name = object_type_name.clone();
            collect_names_from_object_type(object_types, found_type_names, &object_type_name)?;
            found_type_names.insert(object_type_name);
        }
        Type::ArrayOf(t) => collect_names_from_type(object_types, found_type_names, t)?,
        Type::Nullable(t) => collect_names_from_type(object_types, found_type_names, t)?,
        Type::ExtendedJSON => (),
        Type::Scalar(_) => (),
    };
    Ok(())
}

fn collect_names_from_object_type(
    object_types: &BTreeMap<ObjectTypeName, ObjectType>,
    found_type_names: &mut HashSet<ObjectTypeName>,
    input_type_name: &ObjectTypeName,
) -> Result<()> {
    let object_type = object_types
        .get(input_type_name)
        .ok_or_else(|| Error::UnknownObjectType(input_type_name.to_string()))?;
    for (_, field) in object_type.fields.iter() {
        collect_names_from_type(object_types, found_type_names, &field.r#type)?;
    }
    Ok(())
}

fn mk_object_type_name(name: &str) -> ObjectTypeName {
    name.into()
}
