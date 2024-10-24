use configuration::Configuration;
use ndc_models::{CollectionInfo, CollectionName, ObjectTypeName};

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
