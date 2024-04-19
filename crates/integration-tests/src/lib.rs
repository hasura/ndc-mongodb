use std::{collections::HashMap, env};

use anyhow::anyhow;
#[cfg(test)]
use insta::assert_yaml_snapshot;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const ENGINE_GRAPHQL_URL: &str = "ENGINE_GRAPHQL_URL";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_name: Option<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    variables: HashMap<String, Value>,
}

impl From<String> for GraphQLRequest {
    fn from(query: String) -> Self {
        GraphQLRequest {
            query,
            operation_name: Default::default(),
            variables: Default::default(),
        }
    }
}

impl From<&str> for GraphQLRequest {
    fn from(query: &str) -> Self {
        GraphQLRequest {
            query: query.to_owned(),
            operation_name: Default::default(),
            variables: Default::default(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct GraphQLResponse {
    data: Value,
    errors: Option<Vec<Value>>,
}

pub async fn run_query(request: impl Into<GraphQLRequest>) -> anyhow::Result<GraphQLResponse> {
    let graphql_url = get_graphql_url()?;
    let client = Client::new();
    let response = client
        .post(graphql_url)
        .header("x-hasura-role", "admin")
        .json(&request.into())
        .send()
        .await?;
    let graphql_response = response.json().await?;
    Ok(graphql_response)
}

fn get_graphql_url() -> anyhow::Result<String> {
    env::var(ENGINE_GRAPHQL_URL).map_err(|_| anyhow!("please set {ENGINE_GRAPHQL_URL} to the GraphQL endpoint of a running GraphQL Engine server"))
}

#[tokio::test]
async fn runs_a_query() -> anyhow::Result<()> {
    let query = r#"
        query Movies {
          movies(limit: 10) {
            title
            imdb {
              rating
              votes
            }
          }
        }
    "#;
    let response = run_query(query).await?;
    assert_yaml_snapshot!(response);
    Ok(())
}
