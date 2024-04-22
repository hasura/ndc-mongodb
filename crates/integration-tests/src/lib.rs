// Conditionally compile tests based on the "test" and "integration" features. Requiring
// "integration" causes these tests to be skipped when running a workspace-wide `cargo test` which
// is helpful because the integration tests only work with a set of running services.
//
// To run integration tests run, `cargo test --features integration`
#[cfg(all(test, feature = "integration"))]
mod tests;

use std::env;

use anyhow::anyhow;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{to_value, Value};

const ENGINE_GRAPHQL_URL: &str = "ENGINE_GRAPHQL_URL";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<Value>,
}

impl GraphQLRequest {
    pub fn new(query: String) -> Self {
        GraphQLRequest {
            query,
            operation_name: Default::default(),
            variables: Default::default(),
        }
    }

    pub fn operation_name(mut self, name: String) -> Self {
        self.operation_name = Some(name);
        self
    }

    pub fn variables(mut self, vars: impl Serialize) -> Self {
        self.variables = Some(to_value(&vars).unwrap());
        self
    }

    pub async fn run(&self) -> anyhow::Result<GraphQLResponse> {
        let graphql_url = get_graphql_url()?;
        let client = Client::new();
        let response = client
            .post(graphql_url)
            .header("x-hasura-role", "admin")
            .json(self)
            .send()
            .await?;
        let graphql_response = response.json().await?;
        Ok(graphql_response)
    }
}

impl From<String> for GraphQLRequest {
    fn from(query: String) -> Self {
        GraphQLRequest::new(query)
    }
}

impl From<&str> for GraphQLRequest {
    fn from(query: &str) -> Self {
        GraphQLRequest::new(query.to_owned())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GraphQLResponse {
    data: Value,
    errors: Option<Vec<Value>>,
}

pub fn query(q: impl ToString) -> GraphQLRequest {
    q.to_string().into()
}

fn get_graphql_url() -> anyhow::Result<String> {
    env::var(ENGINE_GRAPHQL_URL).map_err(|_| anyhow!("please set {ENGINE_GRAPHQL_URL} to the GraphQL endpoint of a running GraphQL Engine server"))
}
