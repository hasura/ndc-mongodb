use std::collections::BTreeMap;

use configuration::{schema::ObjectType, Configuration};
use futures::future::try_join_all;
use itertools::Itertools;
use mongodb::Database;
use mongodb_agent_common::{
    procedure::Procedure, query::serialization::bson_to_json, state::ConnectorState,
};
use ndc_sdk::{
    connector::MutationError,
    json_response::JsonResponse,
    models::{MutationOperation, MutationOperationResults, MutationRequest, MutationResponse},
};

pub async fn handle_mutation_request(
    config: &Configuration,
    state: &ConnectorState,
    mutation_request: MutationRequest,
) -> Result<JsonResponse<MutationResponse>, MutationError> {
    tracing::debug!(?config, mutation_request = %serde_json::to_string(&mutation_request).unwrap(), "executing mutation");
    let database = state.database();
    let jobs = look_up_procedures(config, mutation_request)?;
    let operation_results = try_join_all(
        jobs.into_iter()
            .map(|procedure| execute_procedure(&config.object_types, database.clone(), procedure)),
    )
    .await?;
    Ok(JsonResponse::Value(MutationResponse { operation_results }))
}

/// Looks up procedures according to the names given in the mutation request, and pairs them with
/// arguments and requested fields. Returns an error if any procedures cannot be found.
fn look_up_procedures(
    config: &Configuration,
    mutation_request: MutationRequest,
) -> Result<Vec<Procedure<'_>>, MutationError> {
    let (procedures, not_found): (Vec<Procedure>, Vec<String>) = mutation_request
        .operations
        .into_iter()
        .map(|operation| match operation {
            MutationOperation::Procedure {
                name, arguments, ..
            } => {
                let native_procedure = config.native_procedures.get(&name);
                native_procedure.ok_or(name).map(|native_procedure| {
                    Procedure::from_native_procedure(native_procedure, arguments)
                })
            }
        })
        .partition_result();

    if !not_found.is_empty() {
        return Err(MutationError::UnprocessableContent(format!(
            "request includes unknown procedures: {}",
            not_found.join(", ")
        )));
    }

    Ok(procedures)
}

async fn execute_procedure(
    object_types: &BTreeMap<String, ObjectType>,
    database: Database,
    procedure: Procedure<'_>,
) -> Result<MutationOperationResults, MutationError> {
    let (result, result_type) = procedure
        .execute(object_types, database.clone())
        .await
        .map_err(|err| MutationError::InvalidRequest(err.to_string()))?;
    let json_result = bson_to_json(&result_type, object_types, result.into())
        .map_err(|err| MutationError::Other(Box::new(err)))?;
    Ok(MutationOperationResults::Procedure {
        result: json_result,
    })
}
