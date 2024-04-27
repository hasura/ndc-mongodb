use std::collections::BTreeMap;

use configuration::Configuration;
use futures::future::try_join_all;
use itertools::Itertools;
use mongodb::Database;
use mongodb_agent_common::{
    procedure::Procedure, query::serialization::bson_to_json, state::ConnectorState,
};
use ndc_sdk::{
    connector::MutationError,
    json_response::JsonResponse,
    models::{
        MutationOperation, MutationOperationResults, MutationRequest, MutationResponse,
        NestedField, Relationship,
    },
};

use crate::{
    api_type_conversions::QueryContext,
    query_response::{extend_configured_object_types, prune_type_to_field_selection},
};

pub async fn handle_mutation_request(
    config: &Configuration,
    query_context: QueryContext<'_>,
    state: &ConnectorState,
    mutation_request: MutationRequest,
) -> Result<JsonResponse<MutationResponse>, MutationError> {
    tracing::debug!(?config, mutation_request = %serde_json::to_string(&mutation_request).unwrap(), "executing mutation");
    let database = state.database();
    let jobs = look_up_procedures(config, &mutation_request)?;
    let operation_results = try_join_all(jobs.into_iter().map(|(procedure, requested_fields)| {
        execute_procedure(
            &query_context,
            database.clone(),
            &mutation_request.collection_relationships,
            procedure,
            requested_fields,
        )
    }))
    .await?;
    Ok(JsonResponse::Value(MutationResponse { operation_results }))
}

/// Looks up procedures according to the names given in the mutation request, and pairs them with
/// arguments and requested fields. Returns an error if any procedures cannot be found.
fn look_up_procedures<'a, 'b>(
    config: &'a Configuration,
    mutation_request: &'b MutationRequest,
) -> Result<Vec<(Procedure<'a>, Option<&'b NestedField>)>, MutationError> {
    let (procedures, not_found): (Vec<_>, Vec<String>) = mutation_request
        .operations
        .iter()
        .map(|operation| match operation {
            MutationOperation::Procedure {
                name,
                arguments,
                fields,
            } => {
                let native_procedure = config.native_procedures.get(name);
                let procedure = native_procedure.ok_or(name).map(|native_procedure| {
                    Procedure::from_native_procedure(native_procedure, arguments.clone())
                })?;
                Ok((procedure, fields.as_ref()))
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
    query_context: &QueryContext<'_>,
    database: Database,
    relationships: &BTreeMap<String, Relationship>,
    procedure: Procedure<'_>,
    requested_fields: Option<&NestedField>,
) -> Result<MutationOperationResults, MutationError> {
    let (result, result_type) = procedure
        .execute(&query_context.object_types, database.clone())
        .await
        .map_err(|err| MutationError::InvalidRequest(err.to_string()))?;

    let (requested_result_type, temp_object_types) = prune_type_to_field_selection(
        query_context,
        relationships,
        &[],
        &result_type,
        requested_fields,
    )
    .map_err(|err| MutationError::Other(Box::new(err)))?;
    let object_types = extend_configured_object_types(query_context, temp_object_types);

    let json_result = bson_to_json(&requested_result_type, &object_types, result.into())
        .map_err(|err| MutationError::Other(Box::new(err)))?;

    Ok(MutationOperationResults::Procedure {
        result: json_result,
    })
}
