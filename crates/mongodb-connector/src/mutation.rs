use std::collections::BTreeMap;

use configuration::native_queries::NativeQuery;
use futures::future::try_join_all;
use itertools::Itertools;
use mongodb::Database;
use mongodb_agent_common::interface_types::MongoConfig;
use ndc_sdk::{
    connector::MutationError,
    json_response::JsonResponse,
    models::{
        MutationOperation, MutationOperationResults, MutationRequest, MutationResponse, NestedField,
    },
};
use serde_json::Value;

/// A procedure combined with inputs
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct Job<'a> {
    // For the time being all procedures are native queries.
    native_query: &'a NativeQuery,
    arguments: BTreeMap<String, Value>,
    fields: Option<NestedField>,
}

impl<'a> Job<'a> {
    pub fn new(
        native_query: &'a NativeQuery,
        arguments: BTreeMap<String, Value>,
        fields: Option<NestedField>,
    ) -> Self {
        Job {
            native_query,
            arguments,
            fields,
        }
    }
}

pub async fn handle_mutation_request(
    config: &MongoConfig,
    mutation_request: MutationRequest,
) -> Result<JsonResponse<MutationResponse>, MutationError> {
    tracing::debug!(?config, mutation_request = %serde_json::to_string(&mutation_request).unwrap(), "executing mutation");
    let database = config.client.database(&config.database);
    let jobs = look_up_procedures(config, mutation_request)?;
    let operation_results = try_join_all(
        jobs.into_iter()
            .map(|job| execute_job(database.clone(), job)),
    )
    .await?;
    Ok(JsonResponse::Value(MutationResponse { operation_results }))
}

/// Looks up procedures according to the names given in the mutation request, and pairs them with
/// arguments and requested fields. Returns an error if any procedures cannot be found.
fn look_up_procedures(
    config: &MongoConfig,
    mutation_request: MutationRequest,
) -> Result<Vec<Job<'_>>, MutationError> {
    let (jobs, not_found): (Vec<Job>, Vec<String>) = mutation_request
        .operations
        .into_iter()
        .map(|operation| match operation {
            MutationOperation::Procedure {
                name,
                arguments,
                fields,
            } => {
                let native_query = config
                    .native_queries
                    .iter()
                    .find(|native_query| native_query.name == name);
                native_query
                    .ok_or(name)
                    .map(|nq| Job::new(nq, arguments, fields))
            }
        })
        .partition_result();

    if !not_found.is_empty() {
        return Err(MutationError::UnprocessableContent(format!(
            "request includes unknown procedures: {}",
            not_found.join(", ")
        )));
    }

    Ok(jobs)
}

async fn execute_job(
    database: Database,
    job: Job<'_>,
) -> Result<MutationOperationResults, MutationError> {
    let result = database
        .run_command(job.native_query.command.clone(), None)
        .await
        .map_err(|err| match *err.kind {
            mongodb::error::ErrorKind::InvalidArgument { message, .. } => {
                MutationError::UnprocessableContent(message)
            }
            err => MutationError::Other(Box::new(err)),
        })?;
    let json_result =
        serde_json::to_value(result).map_err(|err| MutationError::Other(Box::new(err)))?;
    Ok(MutationOperationResults::Procedure {
        result: json_result,
    })
}
