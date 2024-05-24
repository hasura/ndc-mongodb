use ndc_sdk::models::{
    Capabilities, CapabilitiesResponse, LeafCapability, QueryCapabilities, RelationshipCapabilities,
};

pub fn mongo_capabilities_response() -> CapabilitiesResponse {
    ndc_sdk::models::CapabilitiesResponse {
        version: "0.1.2".to_owned(),
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
