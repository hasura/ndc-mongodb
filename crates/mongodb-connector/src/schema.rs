use std::collections::BTreeMap;

use configuration::{metadata, Configuration};
use ndc_sdk::{connector, models};

use crate::capabilities;

pub async fn get_schema(
    config: &Configuration,
) -> Result<models::SchemaResponse, connector::SchemaError> {
    let metadata = &config.metadata;
    let object_types = map_object_types(&metadata.object_types);
    let collections = metadata.collections.iter().map(map_collection).collect();
    Ok(models::SchemaResponse {
        collections,
        object_types,
        scalar_types: capabilities::scalar_types(),
        functions: Default::default(),
        procedures: Default::default(),
    })
}

fn map_object_types(object_types: &[metadata::ObjectType]) -> BTreeMap<String, models::ObjectType> {
    object_types
        .iter()
        .map(|t| {
            (
                t.name.clone(),
                models::ObjectType {
                    fields: map_field_infos(&t.fields),
                    description: t.description.clone(),
                },
            )
        })
        .collect()
}

fn map_field_infos(fields: &[metadata::ObjectField]) -> BTreeMap<String, models::ObjectField> {
    fields
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                models::ObjectField {
                    r#type: map_type(&f.r#type),
                    description: f.description.clone(),
                },
            )
        })
        .collect()
}

fn map_type(t: &metadata::Type) -> models::Type {
    match t {
        metadata::Type::Scalar(t) => models::Type::Named {
            name: t.graphql_name(),
        },
        metadata::Type::Object(t) => models::Type::Named { name: t.clone() },
        metadata::Type::ArrayOf(t) => models::Type::Array {
            element_type: Box::new(map_type(t)),
        },
        metadata::Type::Nullable(t) => models::Type::Nullable {
            underlying_type: Box::new(map_type(t)),
        },
    }
}

fn map_collection(collection: &metadata::Collection) -> models::CollectionInfo {
    models::CollectionInfo {
        name: collection.name.clone(),
        collection_type: collection.r#type.clone(),
        description: collection.description.clone(),
        arguments: Default::default(),
        foreign_keys: Default::default(),
        uniqueness_constraints: Default::default(),
    }
}
