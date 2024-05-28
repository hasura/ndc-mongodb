use std::{collections::BTreeMap, fmt::Display};

use configuration::native_query::NativeQuery;
use ndc_models::Argument;

use crate::mongo_query_plan::{MongoConfiguration, QueryPlan};

#[derive(Clone, Debug)]
pub enum QueryTarget<'a> {
    Collection(String),
    NativeQuery {
        name: String,
        native_query: &'a NativeQuery,
        arguments: &'a BTreeMap<String, Argument>,
    },
}

impl QueryTarget<'_> {
    pub fn for_request<'a>(
        config: &'a MongoConfiguration,
        query_request: &'a QueryPlan,
    ) -> QueryTarget<'a> {
        let collection = &query_request.collection;
        match config.native_queries().get(collection) {
            Some(native_query) => QueryTarget::NativeQuery {
                name: collection.to_owned(),
                native_query,
                arguments: &query_request.arguments,
            },
            None => QueryTarget::Collection(collection.to_owned()),
        }
    }

    pub fn input_collection(&self) -> Option<&str> {
        match self {
            QueryTarget::Collection(collection_name) => Some(collection_name),
            QueryTarget::NativeQuery { native_query, .. } => {
                native_query.input_collection.as_deref()
            }
        }
    }
}

impl Display for QueryTarget<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryTarget::Collection(collection_name) => write!(f, "Collection({collection_name})"),
            QueryTarget::NativeQuery { name, .. } => write!(f, "NativeQuery({name})"),
        }
    }
}
