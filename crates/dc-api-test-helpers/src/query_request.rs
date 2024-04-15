use std::collections::HashMap;

use dc_api_types::{Query, QueryRequest, ScalarValue, TableRelationships, Target, VariableSet};

#[derive(Clone, Debug, Default)]
pub struct QueryRequestBuilder {
    foreach: Option<Vec<HashMap<String, ScalarValue>>>,
    query: Option<Query>,
    target: Option<Target>,
    relationships: Option<Vec<TableRelationships>>,
    variables: Option<Vec<VariableSet>>,
}

pub fn query_request() -> QueryRequestBuilder {
    Default::default()
}

impl QueryRequestBuilder {
    pub fn target<I, S>(mut self, name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: ToString,
    {
        self.target = Some(Target::TTable {
            name: name.into_iter().map(|v| v.to_string()).collect(),
            arguments: Default::default(),
        });
        self
    }

    pub fn query(mut self, query: impl Into<Query>) -> Self {
        self.query = Some(query.into());
        self
    }

    pub fn relationships(mut self, relationships: impl Into<Vec<TableRelationships>>) -> Self {
        self.relationships = Some(relationships.into());
        self
    }
}

impl From<QueryRequestBuilder> for QueryRequest {
    fn from(builder: QueryRequestBuilder) -> Self {
        QueryRequest {
            foreach: builder.foreach.map(Some),
            query: Box::new(
                builder
                    .query
                    .expect("cannot build from a QueryRequestBuilder without a query"),
            ),
            target: builder
                .target
                .expect("cannot build from a QueryRequestBuilder without a target"),
            relationships: builder.relationships.unwrap_or_default(),
            variables: builder.variables,
        }
    }
}
