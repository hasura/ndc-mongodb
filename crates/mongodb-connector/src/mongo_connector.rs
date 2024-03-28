use std::path::Path;

use anyhow::anyhow;
use async_trait::async_trait;
use configuration::Configuration;
use mongodb_agent_common::{
    explain::explain_query, health::check_health, interface_types::MongoConfig,
    query::handle_query_request,
};
use ndc_sdk::{
    connector::{
        Connector, ExplainError, FetchMetricsError, HealthError, InitializationError,
        MutationError, ParseError, QueryError, SchemaError,
    },
    json_response::JsonResponse,
    models::{
        CapabilitiesResponse, ExplainResponse, MutationRequest, MutationResponse, QueryRequest,
        QueryResponse, SchemaResponse,
    },
};
use tracing::instrument;

use crate::{
    api_type_conversions::{
        v2_to_v3_explain_response, v2_to_v3_query_response, v3_to_v2_query_request, QueryContext,
    },
    capabilities::scalar_types,
    error_mapping::{mongo_agent_error_to_explain_error, mongo_agent_error_to_query_error},
};
use crate::{capabilities::mongo_capabilities_response, mutation::handle_mutation_request};

#[derive(Clone, Default)]
pub struct MongoConnector;

#[async_trait]
impl Connector for MongoConnector {
    type Configuration = Configuration;
    type State = MongoConfig;

    #[instrument(err, skip_all)]
    async fn parse_configuration(
        configuration_dir: impl AsRef<Path> + Send,
    ) -> Result<Self::Configuration, ParseError> {
        let configuration = Configuration::parse_configuration(configuration_dir)
            .await
            .map_err(|err| ParseError::Other(err.into()))?;
        Ok(configuration)
    }

    /// Reads database connection URI from environment variable
    #[instrument(err, skip_all)]
    // `instrument` automatically emits traces when this function returns.
    // - `err` limits logging to `Err` results, at log level `error`
    // - `skip_all` omits arguments from the trace
    async fn try_init_state(
        configuration: &Self::Configuration,
        _metrics: &mut prometheus::Registry,
    ) -> Result<Self::State, InitializationError> {
        let state = mongodb_agent_common::state::try_init_state(configuration).await?;
        Ok(state)
    }

    #[instrument(err, skip_all)]
    fn fetch_metrics(
        _configuration: &Self::Configuration,
        _state: &Self::State,
    ) -> Result<(), FetchMetricsError> {
        Ok(())
    }

    #[instrument(err, skip_all)]
    async fn health_check(
        _configuration: &Self::Configuration,
        state: &Self::State,
    ) -> Result<(), HealthError> {
        let status = check_health(state)
            .await
            .map_err(|e| HealthError::Other(e.into()))?;
        match status.as_u16() {
            200..=299 => Ok(()),
            s => Err(HealthError::Other(anyhow!("unhealthy status: {s}").into())),
        }
    }

    async fn get_capabilities() -> JsonResponse<CapabilitiesResponse> {
        mongo_capabilities_response().into()
    }

    #[instrument(err, skip_all)]
    async fn get_schema(
        configuration: &Self::Configuration,
    ) -> Result<JsonResponse<SchemaResponse>, SchemaError> {
        let response = crate::schema::get_schema(configuration).await?;
        Ok(response.into())
    }

    #[instrument(err, skip_all)]
    async fn query_explain(
        _configuration: &Self::Configuration,
        state: &Self::State,
        request: QueryRequest,
    ) -> Result<JsonResponse<ExplainResponse>, ExplainError> {
        let v2_request = v3_to_v2_query_request(
            &QueryContext {
                functions: vec![],
                scalar_types: scalar_types(),
            },
            request,
        )?;
        let response = explain_query(state, v2_request)
            .await
            .map_err(mongo_agent_error_to_explain_error)?;
        Ok(v2_to_v3_explain_response(response).into())
    }

    #[instrument(err, skip_all)]
    async fn mutation_explain(
        _configuration: &Self::Configuration,
        _state: &Self::State,
        _request: MutationRequest,
    ) -> Result<JsonResponse<ExplainResponse>, ExplainError> {
        Err(ExplainError::UnsupportedOperation(
            "The MongoDB agent does not yet support mutations".to_owned(),
        ))
    }

    #[instrument(err, skip_all)]
    async fn mutation(
        _configuration: &Self::Configuration,
        state: &Self::State,
        request: MutationRequest,
    ) -> Result<JsonResponse<MutationResponse>, MutationError> {
        handle_mutation_request(state, request).await
    }

    #[instrument(err, skip_all)]
    async fn query(
        _configuration: &Self::Configuration,
        state: &Self::State,
        request: QueryRequest,
    ) -> Result<JsonResponse<QueryResponse>, QueryError> {
        let v2_request = v3_to_v2_query_request(
            &QueryContext {
                functions: vec![],
                scalar_types: scalar_types(),
            },
            request,
        )?;
        let response_json = handle_query_request(state, v2_request)
            .await
            .map_err(mongo_agent_error_to_query_error)?;

        // TODO: This requires parsing and reserializing the response from MongoDB. We can avoid
        // this by passing a response format enum to the query pipeline builder that will format
        // responses differently for v3 vs v2. MVC-7
        let response = response_json
            .into_value()
            .map_err(|e| QueryError::Other(Box::new(e)))?;

        // TODO: If we are able to push v3 response formatting to the MongoDB aggregation pipeline
        // then we can switch to using `map_unserialized` here to avoid  deserializing and
        // reserializing the response. MVC-7
        Ok(v2_to_v3_query_response(response).into())
    }
}
