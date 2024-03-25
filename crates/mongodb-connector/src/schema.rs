use std::collections::BTreeMap;

use configuration::{
    native_queries::{self, NativeQuery},
    schema, Configuration,
};
use ndc_sdk::{connector, models};

use crate::capabilities;

pub async fn get_schema(
    config: &Configuration,
) -> Result<models::SchemaResponse, connector::SchemaError> {
    let schema = &config.schema;
    let collections = schema.collections.iter().map(map_collection).collect();
    let object_types = config.object_types().map(map_object_type).collect();

    let functions = config
        .native_queries
        .iter()
        .filter(|(_, q)| q.mode == native_queries::Mode::ReadOnly)
        .map(native_query_to_function)
        .collect();

    let procedures = config
        .native_queries
        .iter()
        .filter(|(_, q)| q.mode == native_queries::Mode::ReadWrite)
        .map(native_query_to_procedure)
        .collect();

    Ok(models::SchemaResponse {
        collections,
        object_types,
        scalar_types: capabilities::scalar_types(),
        functions,
        procedures,
    })
}

fn map_object_type(
    (name, object_type): (&String, &schema::ObjectType),
) -> (String, models::ObjectType) {
    (
        name.clone(),
        models::ObjectType {
            fields: map_field_infos(&object_type.fields),
            description: object_type.description.clone(),
        },
    )
}

fn map_field_infos(
    fields: &BTreeMap<String, schema::ObjectField>,
) -> BTreeMap<String, models::ObjectField> {
    fields
        .iter()
        .map(|(name, field)| {
            (
                name.clone(),
                models::ObjectField {
                    r#type: map_type(&field.r#type),
                    description: field.description.clone(),
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

fn map_collection((name, collection): (&String, &schema::Collection)) -> models::CollectionInfo {
    models::CollectionInfo {
        name: name.clone(),
        collection_type: collection.r#type.clone(),
        description: collection.description.clone(),
        arguments: Default::default(),
        foreign_keys: Default::default(),
        uniqueness_constraints: Default::default(),
    }
}

/// For read-only native queries
fn native_query_to_function((query_name, query): (&String, &NativeQuery)) -> models::FunctionInfo {
    let arguments = query
        .arguments
        .iter()
        .map(|(name, field)| {
            (
                name.clone(),
                models::ArgumentInfo {
                    argument_type: map_type(&field.r#type),
                    description: field.description.clone(),
                },
            )
        })
        .collect();
    models::FunctionInfo {
        name: query_name.clone(),
        description: query.description.clone(),
        arguments,
        result_type: map_type(&query.result_type),
    }
}

/// For read-write native queries
fn native_query_to_procedure(
    (query_name, query): (&String, &NativeQuery),
) -> models::ProcedureInfo {
    let arguments = query
        .arguments
        .iter()
        .map(|(name, field)| {
            (
                name.clone(),
                models::ArgumentInfo {
                    argument_type: map_type(&field.r#type),
                    description: field.description.clone(),
                },
            )
        })
        .collect();
    models::ProcedureInfo {
        name: query_name.clone(),
        description: query.description.clone(),
        arguments,
        result_type: map_type(&query.result_type),
    }
}
