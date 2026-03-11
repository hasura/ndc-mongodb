use std::path::Path;

use async_trait::async_trait;
use configuration::Configuration;
use configuration_store::{
    resolve_configuration_mode, ConfigurationMode, PostgresConfigurationStore,
};
use http::StatusCode;
use mongodb_agent_common::{
    explain::explain_query,
    interface_types::MongoAgentError,
    mongo_query_plan::MongoConfiguration,
    query::handle_query_request,
    state::{self, ConnectorState},
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

/// The connector's configuration type. In JSON mode, the full configuration is loaded at startup.
/// In Postgres mode, configuration is fetched on-demand per request from the config store.
#[derive(Clone, Debug)]
pub enum ConnectorConfig {
    /// File-based configuration, fully loaded at startup.
    Static(MongoConfiguration),
    /// Postgres-based configuration, fetched per request.
    Postgres(PostgresConfigurationStore),
}

impl ConnectorConfig {
    /// Resolve configuration for a specific collection by fetching from the config store.
    /// For static mode, returns the already-loaded configuration.
    async fn resolve_for_collection(
        &self,
        collection_name: &str,
    ) -> connector::Result<MongoConfiguration> {
        match self {
            ConnectorConfig::Static(config) => Ok(config.clone()),
            ConnectorConfig::Postgres(store) => {
                let configuration = store
                    .read_collection_configuration(collection_name)
                    .await
                    .map_err(|err| {
                        ErrorResponse::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!(
                                "failed to read configuration for collection {collection_name}: {err:#}"
                            ),
                            json!({}),
                        )
                    })?;
                Ok(MongoConfiguration(configuration))
            }
        }
    }
}

#[derive(Clone, Default)]
pub struct MongoConnector;

#[allow(clippy::blocks_in_conditions)]
#[async_trait]
impl ConnectorSetup for MongoConnector {
    type Connector = MongoConnector;

    #[instrument(err, skip_all)]
    async fn parse_configuration(
        &self,
        configuration_dir: &Path,
    ) -> connector::Result<ConnectorConfig> {
        let mode = resolve_configuration_mode().map_err(|err| {
            ErrorResponse::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to resolve configuration mode: {err:#}"),
                json!({}),
            )
        })?;

        match mode {
            ConfigurationMode::Json => {
                tracing::info!("using file-based configuration");
                let configuration = Configuration::parse_configuration(configuration_dir)
                    .await
                    .map_err(|err| {
                        ErrorResponse::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("{err:#}"),
                            json!({}),
                        )
                    })?;
                tracing::debug!(?configuration);
                Ok(ConnectorConfig::Static(MongoConfiguration(configuration)))
            }
            ConfigurationMode::Postgres {
                url,
                connector_id,
                schema,
            } => {
                tracing::info!(
                    connector_id = %connector_id,
                    schema = %schema,
                    "using postgres-based configuration (on-demand per request)"
                );
                let store =
                    PostgresConfigurationStore::new(url, connector_id, schema).map_err(|err| {
                        ErrorResponse::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("failed to create postgres config store: {err:#}"),
                            json!({}),
                        )
                    })?;
                Ok(ConnectorConfig::Postgres(store))
            }
        }
    }

    /// Reads database connection URI from environment variable, or from postgres config store.
    #[instrument(err, skip_all)]
    async fn try_init_state(
        &self,
        configuration: &ConnectorConfig,
        _metrics: &mut prometheus::Registry,
    ) -> connector::Result<ConnectorState> {
        let connector_state = match configuration {
            ConnectorConfig::Static(_) => state::try_init_state().await?,
            ConnectorConfig::Postgres(store) => {
                let uri = store.read_connection_uri().await.map_err(|err| {
                    ErrorResponse::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to read connection URI from config store: {err:#}"),
                        json!({}),
                    )
                })?;
                let resolved_uri = uri.resolve().map_err(|err| {
                    ErrorResponse::new(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        format!("failed to resolve connection URI: {err:#}"),
                        json!({}),
                    )
                })?;
                state::try_init_state_from_uri(Some(&resolved_uri))
                    .await
                    .map_err(|err| {
                        ErrorResponse::new(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            format!("failed to initialize MongoDB state: {err:#}"),
                            json!({}),
                        )
                    })?
            }
        };

        Ok(connector_state)
    }
}

#[allow(clippy::blocks_in_conditions)]
#[async_trait]
impl Connector for MongoConnector {
    type Configuration = ConnectorConfig;
    type State = ConnectorState;

    fn connector_name() -> &'static str {
        "ndc_mongodb"
    }

    fn connector_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

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
        match configuration {
            ConnectorConfig::Static(config) => {
                let response = crate::schema::get_schema(config).await?;
                Ok(response.into())
            }
            ConnectorConfig::Postgres(_) => {
                // In postgres mode, schema is managed externally.
                // Return an empty schema response.
                Ok(JsonResponse::Value(SchemaResponse {
                    collections: vec![],
                    functions: vec![],
                    procedures: vec![],
                    object_types: Default::default(),
                    scalar_types: Default::default(),
                    capabilities: None,
                    request_arguments: None,
                }))
            }
        }
    }

    #[instrument(err, skip_all)]
    async fn query_explain(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: QueryRequest,
    ) -> connector::Result<JsonResponse<ExplainResponse>> {
        let config = configuration
            .resolve_for_collection(request.collection.as_ref())
            .await?;
        let response = explain_query(&config, state, request)
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
        match configuration {
            ConnectorConfig::Static(config) => {
                let response = handle_mutation_request(config, state, request).await?;
                Ok(response)
            }
            ConnectorConfig::Postgres(_) => Err(ErrorResponse::new(
                StatusCode::NOT_IMPLEMENTED,
                "Mutations are not supported in postgres configuration mode".to_string(),
                json!({}),
            )),
        }
    }

    #[instrument(name = "/query", err, skip_all, fields(internal.visibility = "user"))]
    async fn query(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: QueryRequest,
    ) -> connector::Result<JsonResponse<QueryResponse>> {
        let config = configuration
            .resolve_for_collection(request.collection.as_ref())
            .await?;
        let response = handle_query_request(&config, state, request)
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
