//! Defining a DSL using builders cuts out SO MUCH noise from test cases
#![allow(unused_imports)]

mod aggregates;
mod column_selector;
mod comparison_column;
mod comparison_value;
mod expression;
mod field;
mod query;
mod query_request;

use dc_api_types::{
    ColumnMapping, ColumnSelector, Relationship, RelationshipType, TableRelationships, Target,
};

pub use column_selector::*;
pub use comparison_column::*;
pub use comparison_value::*;
pub use expression::*;
pub use field::*;
pub use query::*;
pub use query_request::*;

#[derive(Clone, Debug)]
pub struct RelationshipBuilder {
    pub column_mapping: ColumnMapping,
    pub relationship_type: RelationshipType,
    pub target: Target,
}

pub fn relationship<const S: usize>(
    target: Target,
    column_mapping: [(ColumnSelector, ColumnSelector); S],
) -> RelationshipBuilder {
    RelationshipBuilder::new(target, column_mapping)
}

impl RelationshipBuilder {
    pub fn new<const S: usize>(
        target: Target,
        column_mapping: [(ColumnSelector, ColumnSelector); S],
    ) -> Self {
        RelationshipBuilder {
            column_mapping: ColumnMapping(column_mapping.into_iter().collect()),
            relationship_type: RelationshipType::Array,
            target,
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
}

impl From<RelationshipBuilder> for Relationship {
    fn from(value: RelationshipBuilder) -> Self {
        Relationship {
            column_mapping: value.column_mapping,
            relationship_type: value.relationship_type,
            target: value.target,
        }
    }
}

pub fn source(name: &str) -> Vec<String> {
    vec![name.to_owned()]
}

pub fn target(name: &str) -> Target {
    Target::TTable {
        name: vec![name.to_owned()],
        arguments: Default::default(),
    }
}

pub fn selector_path<const S: usize>(path_elements: [&str; S]) -> ColumnSelector {
    ColumnSelector::Path(
        path_elements
            .into_iter()
            .map(|e| e.to_owned())
            .collect::<Vec<_>>()
            .try_into()
            .expect("column selector path cannot be empty"),
    )
}

pub fn table_relationships<const S: usize>(
    source_table: Vec<String>,
    relationships: [(&str, impl Into<Relationship>); S],
) -> TableRelationships {
    TableRelationships {
        relationships: relationships
            .into_iter()
            .map(|(name, r)| (name.to_owned(), r.into()))
            .collect(),
        source_table,
    }
}
