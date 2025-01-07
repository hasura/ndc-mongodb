use mongodb_agent_common::{
    mongo_query_plan::MongoConfiguration, scalar_types_capabilities::SCALAR_TYPES,
};
use mongodb_support::BsonScalarType;
use ndc_query_plan::QueryContext as _;
use ndc_sdk::{connector, models as ndc};

pub async fn get_schema(config: &MongoConfiguration) -> connector::Result<ndc::SchemaResponse> {
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
        scalar_types: SCALAR_TYPES.clone(),
        capabilities: Some(ndc::CapabilitySchemaInfo {
            query: Some(ndc::QueryCapabilitiesSchemaInfo {
                aggregates: Some(ndc::AggregateCapabilitiesSchemaInfo {
                    count_scalar_type: BsonScalarType::Long.graphql_name().to_string(),
                }),
            }),
        }),
    };
    tracing::debug!(schema = %serde_json::to_string(&schema).unwrap(), "get_schema");
    Ok(schema)
}
