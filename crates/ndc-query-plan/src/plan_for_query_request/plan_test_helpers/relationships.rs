use std::collections::BTreeMap;

use ndc_models::{RelationshipArgument, RelationshipType};

use crate::{ConnectorTypes, Field, Relationship};

use super::QueryBuilder;

#[derive(Clone, Debug)]
pub struct RelationshipBuilder<T: ConnectorTypes> {
    column_mapping: BTreeMap<ndc_models::FieldName, ndc_models::FieldName>,
    relationship_type: RelationshipType,
    target_collection: ndc_models::CollectionName,
    arguments: BTreeMap<ndc_models::ArgumentName, RelationshipArgument>,
    query: QueryBuilder<T>,
}

pub fn relationship<T: ConnectorTypes>(target: &str) -> RelationshipBuilder<T> {
    RelationshipBuilder::new(target)
}

impl<T: ConnectorTypes> RelationshipBuilder<T> {
    pub fn new(target: &str) -> Self {
        RelationshipBuilder {
            column_mapping: Default::default(),
            relationship_type: RelationshipType::Array,
            target_collection: target.into(),
            arguments: Default::default(),
            query: QueryBuilder::new(),
        }
    }

    pub fn build(self) -> Relationship<T> {
        Relationship {
            column_mapping: self.column_mapping,
            relationship_type: self.relationship_type,
            target_collection: self.target_collection,
            arguments: self.arguments,
            query: self.query.into(),
        }
    }

    pub fn column_mapping(
        mut self,
        column_mapping: impl IntoIterator<Item = (impl ToString, impl ToString)>,
    ) -> Self {
        self.column_mapping = column_mapping
            .into_iter()
            .map(|(source, target)| (source.to_string().into(), target.to_string().into()))
            .collect();
        self
    }

    pub fn relationship_type(mut self, relationship_type: RelationshipType) -> Self {
        self.relationship_type = relationship_type;
        self
    }

    pub fn object_type(mut self) -> Self {
        self.relationship_type = RelationshipType::Object;
        self
    }

    pub fn arguments(mut self, arguments: BTreeMap<ndc_models::ArgumentName, RelationshipArgument>) -> Self {
        self.arguments = arguments;
        self
    }

    pub fn query(mut self, query: QueryBuilder<T>) -> Self {
        self.query = query;
        self
    }

    pub fn fields(
        mut self,
        fields: impl IntoIterator<Item = (impl ToString, impl Into<Field<T>>)>,
    ) -> Self {
        self.query = self.query.fields(fields);
        self
    }
}

impl<T: ConnectorTypes> From<RelationshipBuilder<T>> for Relationship<T> {
    fn from(value: RelationshipBuilder<T>) -> Self {
        value.build()
    }
}
