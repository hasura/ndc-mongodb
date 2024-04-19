use lazy_static::lazy_static;
use std::collections::BTreeMap;

use configuration::Configuration;
use ndc_sdk::{connector::SchemaError, models as ndc};

use crate::capabilities;

lazy_static! {
    pub static ref SCALAR_TYPES: BTreeMap<String, ndc::ScalarType> = capabilities::scalar_types();
}

pub async fn get_schema(config: &Configuration) -> Result<ndc::SchemaResponse, SchemaError> {
    Ok(ndc::SchemaResponse {
        collections: config.collections.values().cloned().collect(),
        functions: config.functions.values().cloned().collect(),
        procedures: config.procedures.values().cloned().collect(),
        object_types: config
            .object_types
            .iter()
            .map(|(name, object_type)| (name.clone(), object_type.clone().into()))
            .collect(),
        scalar_types: SCALAR_TYPES.clone(),
    })
}
