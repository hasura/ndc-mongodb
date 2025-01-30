//! Defining a DSL using builders cuts out SO MUCH noise from test cases
#![allow(unused_imports)]

mod aggregates;
pub use aggregates::*;
mod collection_info;
mod column;
mod comparison_target;
mod comparison_value;
mod exists_in_collection;
mod expressions;
mod field;
mod groups;
mod object_type;
mod order_by;
mod path_element;
mod query_response;
mod relationships;
mod type_helpers;

use std::collections::BTreeMap;

use indexmap::IndexMap;
use ndc_models::{
    Aggregate, Argument, Expression, Field, FieldName, OrderBy, OrderByElement, PathElement, Query,
    QueryRequest, Relationship, RelationshipArgument, RelationshipType,
};

// Export this crate's reference to ndc_models so that we can use this reference in macros.
pub extern crate ndc_models;
pub extern crate smol_str;

pub use collection_info::*;
pub use comparison_target::*;
pub use comparison_value::*;
pub use exists_in_collection::*;
pub use expressions::*;
pub use field::*;
pub use groups::*;
pub use object_type::*;
pub use order_by::*;
pub use path_element::*;
pub use query_response::*;
pub use relationships::*;
pub use type_helpers::*;

#[derive(Clone, Debug, Default)]
pub struct QueryRequestBuilder {
    collection: Option<ndc_models::CollectionName>,
    query: Option<Query>,
    arguments: Option<BTreeMap<ndc_models::ArgumentName, Argument>>,
    collection_relationships: Option<BTreeMap<ndc_models::RelationshipName, Relationship>>,
    variables: Option<Vec<BTreeMap<ndc_models::VariableName, serde_json::Value>>>,
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
        self.collection = Some(collection.to_owned().into());
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
                .map(|(name, arg)| (name.to_owned().into(), arg))
                .collect(),
        );
        self
    }

    pub fn relationships(
        mut self,
        relationships: impl IntoIterator<Item = (impl ToString, impl Into<Relationship>)>,
    ) -> Self {
        self.collection_relationships = Some(
            relationships
                .into_iter()
                .map(|(name, r)| (name.to_string().into(), r.into()))
                .collect(),
        );
        self
    }

    pub fn variables(
        mut self,
        variables: impl IntoIterator<
            Item = impl IntoIterator<Item = (impl ToString, impl Into<serde_json::Value>)>,
        >,
    ) -> Self {
        self.variables = Some(
            variables
                .into_iter()
                .map(|var_map| {
                    var_map
                        .into_iter()
                        .map(|(name, value)| (name.to_string().into(), value.into()))
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
    aggregates: Option<IndexMap<ndc_models::FieldName, Aggregate>>,
    fields: Option<IndexMap<ndc_models::FieldName, Field>>,
    limit: Option<u32>,
    offset: Option<u32>,
    order_by: Option<OrderBy>,
    predicate: Option<Expression>,
    groups: Option<ndc_models::Grouping>,
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
            groups: None,
        }
    }

    pub fn fields<const S: usize>(mut self, fields: [(&str, Field); S]) -> Self {
        self.fields = Some(
            fields
                .into_iter()
                .map(|(name, field)| (name.to_owned().into(), field))
                .collect(),
        );
        self
    }

    pub fn aggregates(
        mut self,
        aggregates: impl IntoIterator<Item = (impl Into<FieldName>, impl Into<Aggregate>)>,
    ) -> Self {
        self.aggregates = Some(
            aggregates
                .into_iter()
                .map(|(name, aggregate)| (name.into(), aggregate.into()))
                .collect(),
        );
        self
    }

    pub fn limit(mut self, n: u32) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn order_by(
        mut self,
        elements: impl IntoIterator<Item = impl Into<OrderByElement>>,
    ) -> Self {
        self.order_by = Some(OrderBy {
            elements: elements.into_iter().map(Into::into).collect(),
        });
        self
    }

    pub fn predicate(mut self, expression: Expression) -> Self {
        self.predicate = Some(expression);
        self
    }

    pub fn groups(mut self, groups: impl Into<ndc_models::Grouping>) -> Self {
        self.groups = Some(groups.into());
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
            groups: value.groups,
        }
    }
}

pub fn empty_expression() -> Expression {
    Expression::Or {
        expressions: vec![],
    }
}
