use std::path::Path;

use async_trait::async_trait;
use configuration::Configuration;
use http::StatusCode;
use mongodb_agent_common::{
    explain::explain_query, interface_types::MongoAgentError, mongo_query_plan::MongoConfiguration,
    query::handle_query_request, state::ConnectorState,
};
use ndc_sdk::{
    connector::{self, Connector, ConnectorSetup, ErrorResponse},
    json_response::JsonResponse,
    models::{
        Capabilities, ExplainResponse, MutationRequest, MutationResponse, QueryRequest,
        QueryResponse, SchemaResponse,
    },
};
use serde_json::json;
use tracing::instrument;

use crate::{capabilities::mongo_capabilities, mutation::handle_mutation_request};

#[derive(Clone, Default)]
pub struct MongoConnector;

#[allow(clippy::blocks_in_conditions)]
#[async_trait]
impl ConnectorSetup for MongoConnector {
    type Connector = MongoConnector;

    #[instrument(err, skip_all)]
    async fn parse_configuration(
        &self,
        configuration_dir: impl AsRef<Path> + Send,
    ) -> connector::Result<MongoConfiguration> {
        let configuration = Configuration::parse_configuration(configuration_dir)
            .await
            .map_err(|err| {
                ErrorResponse::new(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    err.to_string(),
                    json!({}),
                )
            })?;
        tracing::debug!(?configuration);
        Ok(MongoConfiguration(configuration))
    }

    /// Reads database connection URI from environment variable
    #[instrument(err, skip_all)]
    // `instrument` automatically emits traces when this function returns.
    // - `err` limits logging to `Err` results, at log level `error`
    // - `skip_all` omits arguments from the trace
    async fn try_init_state(
        &self,
        _configuration: &MongoConfiguration,
        _metrics: &mut prometheus::Registry,
    ) -> connector::Result<ConnectorState> {
        let state = mongodb_agent_common::state::try_init_state().await?;
        Ok(state)
    }
}

#[allow(clippy::blocks_in_conditions)]
#[async_trait]
impl Connector for MongoConnector {
    type Configuration = MongoConfiguration;
    type State = ConnectorState;

    #[instrument(err, skip_all)]
    fn fetch_metrics(
        _configuration: &Self::Configuration,
        _state: &Self::State,
    ) -> connector::Result<()> {
        Ok(())
    }

    async fn get_capabilities() -> Capabilities {
        mongo_capabilities()
    }

    #[instrument(err, skip_all)]
    async fn get_schema(
        configuration: &Self::Configuration,
    ) -> connector::Result<JsonResponse<SchemaResponse>> {
        let response = crate::schema::get_schema(configuration).await?;
        Ok(response.into())
    }

    #[instrument(err, skip_all)]
    async fn query_explain(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: QueryRequest,
    ) -> connector::Result<JsonResponse<ExplainResponse>> {
        let response = explain_query(configuration, state, request)
            .await
            .map_err(map_mongo_agent_error)?;
        Ok(response.into())
    }

    #[instrument(err, skip_all)]
    async fn mutation_explain(
        _configuration: &Self::Configuration,
        _state: &Self::State,
        _request: MutationRequest,
    ) -> connector::Result<JsonResponse<ExplainResponse>> {
        Err(ErrorResponse::new(
            StatusCode::NOT_IMPLEMENTED,
            "Explain for mutations is not implemented yet".to_string(),
            json!({}),
        ))
    }

    #[instrument(err, skip_all)]
    async fn mutation(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: MutationRequest,
    ) -> connector::Result<JsonResponse<MutationResponse>> {
        let response = handle_mutation_request(configuration, state, request).await?;
        Ok(response)
    }

    #[instrument(name = "/query", err, skip_all, fields(internal.visibility = "user"))]
    async fn query(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: QueryRequest,
    ) -> connector::Result<JsonResponse<QueryResponse>> {
        let response = handle_query_request(configuration, state, request)
            .await
            .map_err(map_mongo_agent_error)?;
        Ok(response.into())
    }
}

fn map_mongo_agent_error(err: MongoAgentError) -> ErrorResponse {
    let (status_code, err_response) = err.status_and_error_response();
    let details = match err_response.details {
        Some(details) => details.into_iter().collect(),
        None => json!({}),
    };
    ErrorResponse::new(status_code, err_response.message, details)
}
