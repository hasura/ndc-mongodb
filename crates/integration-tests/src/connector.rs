use ndc_models::{ErrorResponse, QueryRequest, QueryResponse};
use ndc_test_helpers::QueryRequestBuilder;
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::get_connector_url;

#[derive(Clone, Debug, Serialize)]
#[serde(transparent)]
pub struct ConnectorQueryRequest {
    query_request: QueryRequest,
}

impl ConnectorQueryRequest {
    pub async fn run(&self) -> anyhow::Result<ConnectorQueryResponse> {
        let connector_url = get_connector_url()?;
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

impl From<QueryRequest> for ConnectorQueryRequest {
    fn from(query_request: QueryRequest) -> Self {
        ConnectorQueryRequest { query_request }
    }
}

impl From<QueryRequestBuilder> for ConnectorQueryRequest {
    fn from(builder: QueryRequestBuilder) -> Self {
        let request: QueryRequest = builder.into();
        request.into()
    }
}

pub async fn run_connector_query(
    request: impl Into<ConnectorQueryRequest>,
) -> anyhow::Result<ConnectorQueryResponse> {
    let request: ConnectorQueryRequest = request.into();
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
