use std::collections::BTreeMap;

use mongodb_agent_common::{
    mongo_query_plan::MongoConfiguration, scalar_types_capabilities::SCALAR_TYPES,
};
use mongodb_support::BsonScalarType;
use ndc_query_plan::QueryContext as _;
use ndc_sdk::{connector, models as ndc};

pub async fn get_schema(config: &MongoConfiguration) -> connector::Result<ndc::SchemaResponse> {
    let scalar_types = if config.relational_mode().enabled {
        scalar_types_for_relational_mode()
    } else {
        SCALAR_TYPES.clone()
    };

    let schema = ndc::SchemaResponse {
        collections: config.collections().values().cloned().collect(),
        functions: config
            .functions()
            .values()
            .map(|(f, _)| f)
            .cloned()
            .collect(),
        procedures: config.procedures().values().cloned().collect(),
        object_types: config
            .object_types()
            .iter()
            .map(|(name, object_type)| (name.clone(), object_type.clone()))
            .collect(),
        scalar_types,
        capabilities: Some(ndc::CapabilitySchemaInfo {
            query: Some(ndc::QueryCapabilitiesSchemaInfo {
                aggregates: Some(ndc::AggregateCapabilitiesSchemaInfo {
                    count_scalar_type: BsonScalarType::Int.graphql_name().into(),
                }),
            }),
        }),
        request_arguments: None,
    };
    tracing::debug!(schema = %serde_json::to_string(&schema).unwrap(), "get_schema");
    Ok(schema)
}

/// Returns scalar types with JSON/nested types having String representation.
/// This is used when relational mode is enabled to ensure nested data is
/// serialized as JSON strings for SQL-style query compatibility.
fn scalar_types_for_relational_mode() -> BTreeMap<ndc::ScalarTypeName, ndc::ScalarType> {
    SCALAR_TYPES
        .iter()
        .map(|(name, scalar_type)| {
            let modified_type = match &scalar_type.representation {
                // Convert JSON representation to String for relational mode
                ndc::TypeRepresentation::JSON => ndc::ScalarType {
                    representation: ndc::TypeRepresentation::String,
                    ..scalar_type.clone()
                },
                // Keep other representations as-is
                _ => scalar_type.clone(),
            };
            (name.clone(), modified_type)
        })
        .collect()
}
