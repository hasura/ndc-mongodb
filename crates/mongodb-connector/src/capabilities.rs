use std::collections::BTreeMap;

use mongodb_agent_common::scalar_types_capabilities::scalar_types_capabilities;
use ndc_sdk::models::{
    Capabilities, CapabilitiesResponse, LeafCapability, QueryCapabilities,
    RelationshipCapabilities, ScalarType,
};

use crate::api_type_conversions::v2_to_v3_scalar_type_capabilities;

pub fn mongo_capabilities_response() -> CapabilitiesResponse {
    ndc_sdk::models::CapabilitiesResponse {
        version: "0.1.1".to_owned(),
        capabilities: Capabilities {
            query: QueryCapabilities {
                aggregates: Some(LeafCapability {}),
                variables: Some(LeafCapability {}),
                explain: Some(LeafCapability {}),
            },
            mutation: ndc_sdk::models::MutationCapabilities {
                transactional: None,
                explain: None,
            },
            relationships: Some(RelationshipCapabilities {
                relation_comparisons: None,
                order_by_aggregate: None,
            }),
        },
    }
}

pub fn scalar_types() -> BTreeMap<std::string::String, ScalarType> {
    v2_to_v3_scalar_type_capabilities(scalar_types_capabilities())
}
