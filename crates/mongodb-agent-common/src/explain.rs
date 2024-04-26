use configuration::Configuration;
use dc_api_types::{ExplainResponse, QueryRequest};
use mongodb::bson::{doc, to_bson, Bson};

use crate::{
    interface_types::MongoAgentError,
    query::{self, QueryTarget},
    state::ConnectorState,
};

pub async fn explain_query(
    config: &Configuration,
    state: &ConnectorState,
    query_request: QueryRequest,
) -> Result<ExplainResponse, MongoAgentError> {
    tracing::debug!(query_request = %serde_json::to_string(&query_request).unwrap());

    let db = state.database();

    let pipeline = query::pipeline_for_query_request(config, &query_request)?;
    let pipeline_bson = to_bson(&pipeline)?;

    let aggregate_target = match QueryTarget::for_request(config, &query_request).input_collection()
    {
        Some(collection_name) => Bson::String(collection_name.to_owned()),
        None => Bson::Int32(1),
    };

    let query_command = doc! {
        "aggregate": aggregate_target,
        "pipeline": pipeline_bson,
        "cursor": {},
    };

    let explain_command = doc! {
        "explain": &query_command,
        "verbosity": "allPlansExecution",
    };

    tracing::debug!(explain_command = %serde_json::to_string(&explain_command).unwrap());

    let explain_result = db.run_command(explain_command, None).await?;

    let explanation = serde_json::to_string_pretty(&explain_result)
        .map_err(MongoAgentError::Serialization)?
        .lines()
        .map(String::from)
        .collect();

    let query =
        serde_json::to_string_pretty(&query_command).map_err(MongoAgentError::Serialization)?;

    Ok(ExplainResponse {
        lines: explanation,
        query,
    })
}
