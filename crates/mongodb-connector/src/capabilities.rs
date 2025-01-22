use ndc_sdk::models::{
    AggregateCapabilities, Capabilities, ExistsCapabilities, LeafCapability,
    NestedArrayFilterByCapabilities, NestedFieldCapabilities, NestedFieldFilterByCapabilities,
    QueryCapabilities, RelationshipCapabilities,
};

pub fn mongo_capabilities() -> Capabilities {
    Capabilities {
        query: QueryCapabilities {
            aggregates: Some(AggregateCapabilities {
                filter_by: None,
                group_by: None,
            }),
            variables: Some(LeafCapability {}),
            explain: Some(LeafCapability {}),
            nested_fields: NestedFieldCapabilities {
                filter_by: Some(NestedFieldFilterByCapabilities {
                    nested_arrays: Some(NestedArrayFilterByCapabilities {
                        contains: Some(LeafCapability {}),
                        is_empty: Some(LeafCapability {}),
                    }),
                }),
                order_by: Some(LeafCapability {}),
                aggregates: Some(LeafCapability {}),
                nested_collections: None, // TODO: ENG-1464
            },
            exists: ExistsCapabilities {
                named_scopes: None, // TODO: ENG-1487
                unrelated: Some(LeafCapability {}),
                nested_collections: Some(LeafCapability {}),
                nested_scalar_collections: None, // TODO: ENG-1488
            },
        },
        mutation: ndc_sdk::models::MutationCapabilities {
            transactional: None,
            explain: None,
        },
        relationships: Some(RelationshipCapabilities {
            relation_comparisons: Some(LeafCapability {}),
            order_by_aggregate: None,
            nested: None, // TODO: ENG-1490
        }),
    }
}
