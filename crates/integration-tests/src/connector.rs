use ndc_models::{ErrorResponse, QueryRequest, QueryResponse};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{get_connector_chinook_url, get_connector_test_cases_url, get_connector_url};

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct ConnectorQueryRequest {
    #[serde(skip)]
    connector: Connector,
    query_request: QueryRequest,
}

#[derive(Clone, Copy, Debug)]
pub enum Connector {
    Chinook,
    SampleMflix,
    TestCases,
}

impl Connector {
    fn url(self) -> anyhow::Result<Url> {
        match self {
            Connector::Chinook => get_connector_chinook_url(),
            Connector::SampleMflix => get_connector_url(),
            Connector::TestCases => get_connector_test_cases_url(),
        }
    }
}

impl ConnectorQueryRequest {
    pub async fn run(&self) -> anyhow::Result<ConnectorQueryResponse> {
        let connector_url = self.connector.url()?;
        let client = Client::new();
        let response = client
            .post(connector_url.join("query")?)
            .header("x-hasura-role", "admin")
            .json(self)
            .send()
            .await?;
        let query_response = response.json().await?;
        Ok(query_response)
    }
}

pub async fn run_connector_query(
    connector: Connector,
    request: impl Into<QueryRequest>,
) -> anyhow::Result<ConnectorQueryResponse> {
    let request = ConnectorQueryRequest {
        connector,
        query_request: request.into(),
    };
    request.run().await
}

// Using a custom Result-like enum because we need untagged deserialization
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ConnectorQueryResponse {
    Ok(QueryResponse),
    Err(ErrorResponse),
}

impl ConnectorQueryResponse {
    pub fn into_result(self) -> Result<QueryResponse, ErrorResponse> {
        match self {
            ConnectorQueryResponse::Ok(resp) => Ok(resp),
            ConnectorQueryResponse::Err(err) => Err(err),
        }
    }
}

impl From<ConnectorQueryResponse> for Result<QueryResponse, ErrorResponse> {
    fn from(value: ConnectorQueryResponse) -> Self {
        value.into_result()
    }
}
