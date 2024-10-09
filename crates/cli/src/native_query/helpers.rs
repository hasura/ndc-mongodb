use configuration::{schema::Type, Configuration};
use ndc_models::{CollectionInfo, CollectionName, FieldName, ObjectTypeName};

use super::{
    error::{Error, Result},
    pipeline_type_context::PipelineTypeContext,
};

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

/// Looks up the given object type, and traverses the given field path to get the type of the
/// referenced field. If `nested_path` is empty returns the type of the original object.
pub fn nested_field_type<'a>(
    context: &PipelineTypeContext<'_>,
    object_type_name: String,
    nested_path: impl IntoIterator<Item = &'a FieldName>,
) -> Result<Type> {
    let mut parent_type = Type::Object(object_type_name);
    for path_component in nested_path {
        if let Type::Object(type_name) = parent_type {
            let object_type = context
                .get_object_type(&type_name.clone().into())
                .ok_or_else(|| Error::UnknownObjectType(type_name.clone()))?;
            let field = object_type.fields.get(path_component).ok_or_else(|| {
                Error::ObjectMissingField {
                    object_type: type_name.into(),
                    field_name: path_component.clone(),
                }
            })?;
            parent_type = field.r#type.clone();
        }
    }
    Ok(parent_type)
}
