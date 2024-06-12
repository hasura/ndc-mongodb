use std::collections::BTreeMap;

use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{to_value, Value};

use crate::get_graphql_url;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphQLRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    operation_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    variables: Option<Value>,
    #[serde(skip_serializing)]
    headers: BTreeMap<String, String>,
}

impl GraphQLRequest {
    pub fn new(query: String) -> Self {
        GraphQLRequest {
            query,
            operation_name: Default::default(),
            variables: Default::default(),
            headers: [("x-hasura-role".into(), "admin".into())].into(),
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

    pub fn headers(
        mut self,
        headers: impl IntoIterator<Item = (impl ToString, impl ToString)>,
    ) -> Self {
        self.headers = headers
            .into_iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        self
    }

    pub async fn run(&self) -> anyhow::Result<GraphQLResponse> {
        let graphql_url = get_graphql_url()?;
        let client = Client::new();
        let mut request_builder = client.post(graphql_url).json(self);
        for (key, value) in self.headers.iter() {
            request_builder = request_builder.header(key, value);
        }
        let response = request_builder.send().await?;
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

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct GraphQLResponse {
    pub data: Value,
    pub errors: Option<Vec<Value>>,
}

pub fn graphql_query(q: impl ToString) -> GraphQLRequest {
    q.to_string().into()
}
