use std::{borrow::Cow, collections::BTreeMap};

use configuration::Configuration;
use ndc_models::{CollectionInfo, CollectionName, FieldName, ObjectTypeName};
use nonempty::NonEmpty;
use regex::Regex;

use super::error::{Error, Result};

fn find_collection<'a>(
    configuration: &'a Configuration,
    collection_name: &CollectionName,
) -> Result<&'a CollectionInfo> {
    if let Some(collection) = configuration.collections.get(collection_name) {
        return Ok(collection);
    }
    if let Some((_, function)) = configuration.functions.get(collection_name) {
        return Ok(function);
    }

    Err(Error::UnknownCollection(collection_name.to_string()))
}

pub fn find_collection_object_type(
    configuration: &Configuration,
    collection_name: &CollectionName,
) -> Result<ObjectTypeName> {
    let collection = find_collection(configuration, collection_name)?;
    Ok(collection.collection_type.clone())
}

pub fn unique_type_name<A, B>(
    object_types: &BTreeMap<ObjectTypeName, A>,
    added_object_types: &BTreeMap<ObjectTypeName, B>,
    desired_type_name: &str,
) -> ObjectTypeName {
    let (name, mut counter) = parse_counter_suffix(desired_type_name);
    let mut type_name: ObjectTypeName = name.as_ref().into();
    while object_types.contains_key(&type_name) || added_object_types.contains_key(&type_name) {
        counter += 1;
        type_name = format!("{desired_type_name}_{counter}").into();
    }
    type_name
}

/// [unique_type_name] adds a `_n` numeric suffix where necessary. There are cases where we go
/// through multiple layers of unique names. Instead of accumulating multiple suffixes, we can
/// increment the existing suffix. If there is no suffix then the count starts at zero.
pub fn parse_counter_suffix(name: &str) -> (Cow<'_, str>, u32) {
    let re = Regex::new(r"^(.*?)_(\d+)$").unwrap();
    let Some(captures) = re.captures(name) else {
        return (Cow::Borrowed(name), 0);
    };
    let prefix = captures.get(1).unwrap().as_str();
    let Some(count) = captures.get(2).and_then(|s| s.as_str().parse().ok()) else {
        return (Cow::Borrowed(name), 0);
    };
    (Cow::Owned(prefix.to_string()), count)
}

pub fn get_object_field_type<'a>(
    object_types: &'a BTreeMap<ObjectTypeName, ndc_models::ObjectType>,
    object_type_name: &ObjectTypeName,
    object_type: &'a ndc_models::ObjectType,
    path: NonEmpty<FieldName>,
) -> Result<&'a ndc_models::Type> {
    let field_name = path.head;
    let rest = NonEmpty::from_vec(path.tail);

    let field = object_type
        .fields
        .get(&field_name)
        .ok_or_else(|| Error::ObjectMissingField {
            object_type: object_type_name.clone(),
            field_name: field_name.clone(),
        })?;

    match rest {
        None => Ok(&field.r#type),
        Some(rest) => match &field.r#type {
            ndc_models::Type::Named { name } => {
                let type_name: ObjectTypeName = name.clone().into();
                let inner_object_type = object_types
                    .get(&type_name)
                    .ok_or_else(|| Error::UnknownObjectType(type_name.to_string()))?;
                get_object_field_type(object_types, &type_name, inner_object_type, rest)
            }
            _ => Err(Error::ObjectMissingField {
                object_type: object_type_name.clone(),
                field_name: field_name.clone(),
            }),
        },
    }
}
