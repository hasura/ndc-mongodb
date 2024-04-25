//! Defining a DSL using builders cuts out SO MUCH noise from test cases
#![allow(unused_imports)]

mod aggregates;
mod collection_info;
mod comparison_target;
mod comparison_value;
mod exists_in_collection;
mod expressions;
mod field;
mod types;

use std::collections::BTreeMap;

use indexmap::IndexMap;
use ndc_models::{
    Aggregate, Argument, Expression, Field, OrderBy, OrderByElement, PathElement, Query,
    QueryRequest, Relationship, RelationshipArgument, RelationshipType,
};

pub use collection_info::*;
pub use comparison_target::*;
pub use comparison_value::*;
pub use exists_in_collection::*;
pub use expressions::*;
pub use field::*;
pub use types::*;

#[derive(Clone, Debug, Default)]
pub struct QueryRequestBuilder {
    collection: Option<String>,
    query: Option<Query>,
    arguments: Option<BTreeMap<String, Argument>>,
    collection_relationships: Option<BTreeMap<String, Relationship>>,
    variables: Option<Vec<BTreeMap<String, serde_json::Value>>>,
}

pub fn query_request() -> QueryRequestBuilder {
    QueryRequestBuilder::new()
}

impl QueryRequestBuilder {
    pub fn new() -> Self {
        QueryRequestBuilder {
            collection: None,
            query: None,
            arguments: None,
            collection_relationships: None,
            variables: None,
        }
    }

    pub fn collection(mut self, collection: &str) -> Self {
        self.collection = Some(collection.to_owned());
        self
    }

    pub fn query(mut self, query: impl Into<Query>) -> Self {
        self.query = Some(query.into());
        self
    }

    pub fn arguments<const S: usize>(mut self, arguments: [(&str, Argument); S]) -> Self {
        self.arguments = Some(
            arguments
                .into_iter()
                .map(|(name, arg)| (name.to_owned(), arg))
                .collect(),
        );
        self
    }

    pub fn relationships<const S: usize>(
        mut self,
        relationships: [(&str, impl Into<Relationship>); S],
    ) -> Self {
        self.collection_relationships = Some(
            relationships
                .into_iter()
                .map(|(name, r)| (name.to_owned(), r.into()))
                .collect(),
        );
        self
    }

    pub fn variables<const S: usize>(
        mut self,
        variables: [Vec<(&str, serde_json::Value)>; S],
    ) -> Self {
        self.variables = Some(
            variables
                .into_iter()
                .map(|var_map| {
                    var_map
                        .into_iter()
                        .map(|(name, value)| (name.to_owned(), value))
                        .collect()
                })
                .collect(),
        );
        self
    }
}

impl From<QueryRequestBuilder> for QueryRequest {
    fn from(value: QueryRequestBuilder) -> Self {
        QueryRequest {
            collection: value
                .collection
                .expect("cannot build from a QueryRequestBuilder without a collection"),
            query: value
                .query
                .expect("cannot build from a QueryRequestBuilder without a query"),
            arguments: value.arguments.unwrap_or_default(),
            collection_relationships: value.collection_relationships.unwrap_or_default(),
            variables: value.variables,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct QueryBuilder {
    aggregates: Option<IndexMap<String, Aggregate>>,
    fields: Option<IndexMap<String, Field>>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<OrderBy>,
    predicate: Option<Expression>,
}

pub fn query() -> QueryBuilder {
    QueryBuilder::new()
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self {
            fields: None,
            aggregates: Default::default(),
            limit: None,
            offset: None,
            order_by: None,
            predicate: None,
        }
    }

    pub fn fields<const S: usize>(mut self, fields: [(&str, Field); S]) -> Self {
        self.fields = Some(
            fields
                .into_iter()
                .map(|(name, field)| (name.to_owned(), field))
                .collect(),
        );
        self
    }

    pub fn aggregates<const S: usize>(mut self, aggregates: [(&str, Aggregate); S]) -> Self {
        self.aggregates = Some(
            aggregates
                .into_iter()
                .map(|(name, aggregate)| (name.to_owned(), aggregate))
                .collect(),
        );
        self
    }

    pub fn order_by(mut self, elements: Vec<OrderByElement>) -> Self {
        self.order_by = Some(OrderBy { elements });
        self
    }

    pub fn predicate(mut self, expression: Expression) -> Self {
        self.predicate = Some(expression);
        self
    }
}

impl From<QueryBuilder> for Query {
    fn from(value: QueryBuilder) -> Self {
        Query {
            aggregates: value.aggregates,
            fields: value.fields,
            limit: value.limit,
            offset: value.offset,
            order_by: value.order_by,
            predicate: value.predicate,
        }
    }
}

pub fn empty_expression() -> Expression {
    Expression::Or {
        expressions: vec![],
    }
}

#[derive(Clone, Debug)]
pub struct RelationshipBuilder {
    column_mapping: BTreeMap<String, String>,
    relationship_type: RelationshipType,
    target_collection: String,
    arguments: BTreeMap<String, RelationshipArgument>,
}

pub fn relationship<const S: usize>(
    target: &str,
    column_mapping: [(&str, &str); S],
) -> RelationshipBuilder {
    RelationshipBuilder::new(target, column_mapping)
}

impl RelationshipBuilder {
    pub fn new<const S: usize>(target: &str, column_mapping: [(&str, &str); S]) -> Self {
        RelationshipBuilder {
            column_mapping: column_mapping
                .into_iter()
                .map(|(source, target)| (source.to_owned(), target.to_owned()))
                .collect(),
            relationship_type: RelationshipType::Array,
            target_collection: target.to_owned(),
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

    pub fn arguments(mut self, arguments: BTreeMap<String, RelationshipArgument>) -> Self {
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

#[derive(Clone, Debug)]
pub struct PathElementBuilder {
    relationship: String,
    arguments: Option<BTreeMap<String, RelationshipArgument>>,
    predicate: Option<Box<Expression>>,
}

pub fn path_element(relationship: &str) -> PathElementBuilder {
    PathElementBuilder::new(relationship)
}

impl PathElementBuilder {
    pub fn new(relationship: &str) -> Self {
        PathElementBuilder {
            relationship: relationship.to_owned(),
            arguments: None,
            predicate: None,
        }
    }

    pub fn predicate(mut self, expression: Expression) -> Self {
        self.predicate = Some(Box::new(expression));
        self
    }
}

impl From<PathElementBuilder> for PathElement {
    fn from(value: PathElementBuilder) -> Self {
        PathElement {
            relationship: value.relationship,
            arguments: value.arguments.unwrap_or_default(),
            predicate: value.predicate,
        }
    }
}
