use std::{collections::HashMap, fmt::Display};

use configuration::native_query::NativeQuery;
use dc_api_types::{Argument, QueryRequest};

use super::QueryConfig;

#[derive(Clone, Debug)]
pub enum QueryTarget<'a> {
    Collection(String),
    NativeQuery {
        name: String,
        native_query: &'a NativeQuery,
        arguments: &'a HashMap<String, Argument>,
    },
}

impl QueryTarget<'_> {
    pub fn for_request<'a>(
        config: QueryConfig<'a>,
        query_request: &'a QueryRequest,
    ) -> QueryTarget<'a> {
        let target = &query_request.target;
        let target_name = target.name().join(".");
        match config.native_queries.get(&target_name) {
            Some(native_query) => QueryTarget::NativeQuery {
                name: target_name,
                native_query,
                arguments: target.arguments(),
            },
            None => QueryTarget::Collection(target_name),
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
