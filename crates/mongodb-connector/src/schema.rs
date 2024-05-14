use configuration::Configuration;
use mongodb_agent_common::scalar_types_capabilities::SCALAR_TYPES;
use ndc_sdk::{connector::SchemaError, models as ndc};

pub async fn get_schema(config: &Configuration) -> Result<ndc::SchemaResponse, SchemaError> {
    Ok(ndc::SchemaResponse {
        collections: config.collections.values().cloned().collect(),
        functions: config.functions.values().map(|(f, _)| f).cloned().collect(),
        procedures: config.procedures.values().cloned().collect(),
        object_types: config
            .object_types
            .iter()
            .map(|(name, object_type)| (name.clone(), object_type.clone().into()))
            .collect(),
        scalar_types: SCALAR_TYPES.clone(),
    })
}
