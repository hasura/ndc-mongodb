use std::{collections::BTreeMap, fmt::Display};

use configuration::native_query::NativeQuery;
use dc_api_types::QueryRequest;

#[derive(Clone, Debug)]
pub enum QueryTarget<'a> {
    Collection(String),
    NativeQuery {
        name: String,
        native_query: &'a NativeQuery,
    },
}

impl QueryTarget<'_> {
    pub fn for_request<'a>(
        query_request: &QueryRequest,
        native_queries: &'a BTreeMap<String, NativeQuery>,
    ) -> QueryTarget<'a> {
        let target = &query_request.target;
        let target_name = target.name().join(".");
        match native_queries.get(&target_name) {
            Some(native_query) => QueryTarget::NativeQuery {
                name: target_name,
                native_query,
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
