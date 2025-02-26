use std::collections::BTreeMap;

use mongodb::bson::{doc, to_bson, Bson};
use mongodb_support::aggregate::AggregateCommand;
use ndc_models::{ExplainResponse, QueryRequest};
use ndc_query_plan::plan_for_query_request;

use crate::{
    interface_types::MongoAgentError, mongo_query_plan::MongoConfiguration, query,
    state::ConnectorState,
};

pub async fn explain_query(
    config: &MongoConfiguration,
    state: &ConnectorState,
    query_request: QueryRequest,
) -> Result<ExplainResponse, MongoAgentError> {
    let db = state.database();
    let query_plan = plan_for_query_request(config, query_request)?;

    let AggregateCommand {
        collection,
        pipeline,
        let_vars,
    } = query::command_for_query_request(config, &query_plan)?;
    let pipeline_bson = to_bson(&pipeline)?;

    let aggregate_target = match collection {
        Some(collection_name) => Bson::String(collection_name.to_string()),
        None => Bson::Int32(1),
    };

    let query_command = {
        let mut cmd = doc! {
            "aggregate": aggregate_target,
            "pipeline": pipeline_bson,
            "cursor": {},
        };
        if let Some(let_vars) = let_vars {
            cmd.insert("let", let_vars);
        }
        cmd
    };

    let explain_command = doc! {
        "explain": &query_command,
        "verbosity": "allPlansExecution",
    };

    tracing::debug!(explain_command = %serde_json::to_string(&explain_command).unwrap());

    let explain_result = db.run_command(explain_command).await?;

    let plan =
        serde_json::to_string_pretty(&explain_result).map_err(MongoAgentError::Serialization)?;

    let query =
        serde_json::to_string_pretty(&query_command).map_err(MongoAgentError::Serialization)?;

    Ok(ExplainResponse {
        details: BTreeMap::from_iter([("plan".to_owned(), plan), ("query".to_owned(), query)]),
    })
}
