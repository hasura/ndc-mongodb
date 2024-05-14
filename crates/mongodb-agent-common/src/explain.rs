use std::collections::BTreeMap;

use configuration::Configuration;
use mongodb::bson::{doc, to_bson, Bson};
use ndc_models::{ExplainResponse, QueryRequest};
use ndc_query_plan::plan_for_query_request;

use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::get_query_context,
    query::{self, QueryTarget},
    state::ConnectorState,
};

pub async fn explain_query(
    config: &Configuration,
    state: &ConnectorState,
    query_request: QueryRequest,
) -> Result<ExplainResponse, MongoAgentError> {
    let db = state.database();
    let query_context = get_query_context(config);
    let query_plan = plan_for_query_request(&query_context, query_request)?;

    let pipeline = query::pipeline_for_query_request(config, &query_plan)?;
    let pipeline_bson = to_bson(&pipeline)?;

    let aggregate_target = match QueryTarget::for_request(config, &query_plan).input_collection() {
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

    let plan =
        serde_json::to_string_pretty(&explain_result).map_err(MongoAgentError::Serialization)?;

    let query =
        serde_json::to_string_pretty(&query_command).map_err(MongoAgentError::Serialization)?;

    Ok(ExplainResponse {
        details: BTreeMap::from_iter([("plan".to_owned(), plan), ("query".to_owned(), query)]),
    })
}
