use std::collections::BTreeMap;

use ndc_models::{Relationship, RelationshipArgument, RelationshipType};

#[derive(Clone, Debug)]
pub struct RelationshipBuilder {
    column_mapping: BTreeMap<ndc_models::FieldName, Vec<ndc_models::FieldName>>,
    relationship_type: RelationshipType,
    target_collection: ndc_models::CollectionName,
    arguments: BTreeMap<ndc_models::ArgumentName, RelationshipArgument>,
}

pub fn relationship<const S: usize>(
    target: &str,
    column_mapping: [(&str, &[&str]); S],
) -> RelationshipBuilder {
    RelationshipBuilder::new(target, column_mapping)
}

impl RelationshipBuilder {
    pub fn new<const S: usize>(target: &str, column_mapping: [(&str, &[&str]); S]) -> Self {
        RelationshipBuilder {
            column_mapping: column_mapping
                .into_iter()
                .map(|(source, target)| {
                    (
                        source.to_owned().into(),
                        target.iter().map(|s| s.to_owned().into()).collect(),
                    )
                })
                .collect(),
            relationship_type: RelationshipType::Array,
            target_collection: target.to_owned().into(),
            arguments: Default::default(),
        }
    }

    pub fn relationship_type(mut self, relationship_type: RelationshipType) -> Self {
        self.relationship_type = relationship_type;
        self
    }

    pub fn object_type(mut self) -> Self {
        self.relationship_type = RelationshipType::Object;
        self
    }

    pub fn arguments(
        mut self,
        arguments: BTreeMap<ndc_models::ArgumentName, RelationshipArgument>,
    ) -> Self {
        self.arguments = arguments;
        self
    }
}

impl From<RelationshipBuilder> for Relationship {
    fn from(value: RelationshipBuilder) -> Self {
        Relationship {
            column_mapping: value.column_mapping,
            relationship_type: value.relationship_type,
            target_collection: value.target_collection,
            arguments: value.arguments,
        }
    }
}

pub fn collection_relationships<const S: usize>(
    relationships: [(&str, impl Into<Relationship>); S],
) -> BTreeMap<String, Relationship> {
    relationships
        .into_iter()
        .map(|(name, r)| (name.to_owned(), r.into()))
        .collect()
}
