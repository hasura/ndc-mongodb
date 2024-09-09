use ndc_sdk::models::{
    Capabilities, ExistsCapabilities, LeafCapability, NestedFieldCapabilities, QueryCapabilities,
    RelationshipCapabilities,
};

pub fn mongo_capabilities() -> Capabilities {
    Capabilities {
        query: QueryCapabilities {
            aggregates: Some(LeafCapability {}),
            variables: Some(LeafCapability {}),
            explain: Some(LeafCapability {}),
            nested_fields: NestedFieldCapabilities {
                filter_by: Some(LeafCapability {}),
                order_by: Some(LeafCapability {}),
                aggregates: None,
            },
            exists: ExistsCapabilities {
                nested_collections: Some(LeafCapability {}),
            },
        },
        mutation: ndc_sdk::models::MutationCapabilities {
            transactional: None,
            explain: None,
        },
        relationships: Some(RelationshipCapabilities {
            relation_comparisons: Some(LeafCapability {}),
            order_by_aggregate: None,
        }),
    }
}
