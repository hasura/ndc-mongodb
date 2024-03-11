use dc_api_types::{ExplainResponse, QueryRequest};
use mongodb::bson::{doc, to_bson};

use crate::{
    interface_types::{MongoAgentError, MongoConfig},
    query::{self, collection_name},
};

pub async fn explain_query(
    config: &MongoConfig,
    query_request: QueryRequest,
) -> Result<ExplainResponse, MongoAgentError> {
    tracing::debug!(query_request = %serde_json::to_string(&query_request).unwrap());

    let db = config.client.database(&config.database);

    let (pipeline, _) = query::pipeline_for_query_request(&query_request)?;
    let pipeline_bson = to_bson(&pipeline)?;

    let query_command = doc! {
        "aggregate": collection_name(&query_request.target),
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
