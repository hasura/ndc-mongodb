use std::collections::BTreeMap;

use configuration::{native_queries::NativeQuery, schema, Configuration};
use ndc_sdk::{connector, models};

use crate::capabilities;

pub async fn get_schema(
    config: &Configuration,
) -> Result<models::SchemaResponse, connector::SchemaError> {
    let schema = &config.schema;
    let object_types = map_object_types(&schema.object_types);
    let configured_collections = schema.collections.iter().map(map_collection);
    let native_queries = config.native_queries.iter().map(map_native_query);

    Ok(models::SchemaResponse {
        collections: configured_collections.chain(native_queries).collect(),
        object_types,
        scalar_types: capabilities::scalar_types(),
        functions: Default::default(),
        procedures: Default::default(),
    })
}

fn map_object_types(object_types: &[schema::ObjectType]) -> BTreeMap<String, models::ObjectType> {
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

fn map_field_infos(fields: &[schema::ObjectField]) -> BTreeMap<String, models::ObjectField> {
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

fn map_type(t: &schema::Type) -> models::Type {
    match t {
        schema::Type::Scalar(t) => models::Type::Named {
            name: t.graphql_name(),
        },
        schema::Type::Object(t) => models::Type::Named { name: t.clone() },
        schema::Type::ArrayOf(t) => models::Type::Array {
            element_type: Box::new(map_type(t)),
        },
        schema::Type::Nullable(t) => models::Type::Nullable {
            underlying_type: Box::new(map_type(t)),
        },
    }
}

fn map_collection(collection: &schema::Collection) -> models::CollectionInfo {
    models::CollectionInfo {
        name: collection.name.clone(),
        collection_type: collection.r#type.clone(),
        description: collection.description.clone(),
        arguments: Default::default(),
        foreign_keys: Default::default(),
        uniqueness_constraints: Default::default(),
    }
}

fn map_native_query(query: &NativeQuery) -> models::CollectionInfo {
    let arguments = query
        .arguments
        .iter()
        .map(|field| {
            (
                field.name.clone(),
                models::ArgumentInfo {
                    argument_type: map_type(&field.r#type),
                    description: field.description.clone(),
                },
            )
        })
        .collect();
    models::CollectionInfo {
        name: query.name.clone(),
        collection_type: query.result_type.clone(),
        uniqueness_constraints: Default::default(),
        foreign_keys: Default::default(),
        description: query.description.clone(),
        arguments,
    }
}
