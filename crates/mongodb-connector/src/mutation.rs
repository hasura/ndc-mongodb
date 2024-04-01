use futures::future::try_join_all;
use itertools::Itertools;
use mongodb::Database;
use mongodb_agent_common::{
    interface_types::MongoConfig, procedure::Procedure, query::serialization::bson_to_json,
};
use ndc_sdk::{
    connector::MutationError,
    json_response::JsonResponse,
    models::{MutationOperation, MutationOperationResults, MutationRequest, MutationResponse},
};

pub async fn handle_mutation_request(
    config: &MongoConfig,
    mutation_request: MutationRequest,
) -> Result<JsonResponse<MutationResponse>, MutationError> {
    tracing::debug!(?config, mutation_request = %serde_json::to_string(&mutation_request).unwrap(), "executing mutation");
    let database = config.client.database(&config.database);
    let jobs = look_up_procedures(config, mutation_request)?;
    let operation_results = try_join_all(
        jobs.into_iter()
            .map(|procedure| execute_procedure(database.clone(), procedure)),
    )
    .await?;
    Ok(JsonResponse::Value(MutationResponse { operation_results }))
}

/// Looks up procedures according to the names given in the mutation request, and pairs them with
/// arguments and requested fields. Returns an error if any procedures cannot be found.
fn look_up_procedures(
    config: &MongoConfig,
    mutation_request: MutationRequest,
) -> Result<Vec<Procedure<'_>>, MutationError> {
    let (procedures, not_found): (Vec<Procedure>, Vec<String>) = mutation_request
        .operations
        .into_iter()
        .map(|operation| match operation {
            MutationOperation::Procedure {
                name, arguments, ..
            } => {
                let native_query = config.native_queries.get(&name);
                native_query.ok_or(name).map(|native_query| {
                    Procedure::from_native_query(native_query, &config.object_types, arguments)
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
    database: Database,
    procedure: Procedure<'_>,
) -> Result<MutationOperationResults, MutationError> {
    let result = procedure
        .execute(database.clone())
        .await
        .map_err(|err| MutationError::InvalidRequest(err.to_string()))?;
    let json_result =
        bson_to_json(result.into()).map_err(|err| MutationError::Other(Box::new(err)))?;
    Ok(MutationOperationResults::Procedure {
        result: json_result,
    })
}
