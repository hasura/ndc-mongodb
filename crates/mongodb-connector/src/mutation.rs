use futures::future::try_join_all;
use itertools::Itertools;
use mongodb::{
    bson::{self, Bson},
    Database,
};
use mongodb_agent_common::{
    mongo_query_plan::MongoConfiguration,
    procedure::Procedure,
    query::{response::type_for_nested_field, serialization::bson_to_json},
    state::ConnectorState,
};
use ndc_query_plan::type_annotated_nested_field;
use ndc_sdk::{
    connector::MutationError,
    json_response::JsonResponse,
    models::{
        self as ndc, MutationOperation, MutationOperationResults, MutationRequest,
        MutationResponse, NestedField, NestedObject,
    },
};

use crate::error_mapping::error_response;

pub async fn handle_mutation_request(
    config: &MongoConfiguration,
    state: &ConnectorState,
    mutation_request: MutationRequest,
) -> Result<JsonResponse<MutationResponse>, MutationError> {
    tracing::debug!(?config, mutation_request = %serde_json::to_string(&mutation_request).unwrap(), "executing mutation");
    let database = state.database();
    let jobs = look_up_procedures(config, &mutation_request)?;
    let operation_results = try_join_all(jobs.into_iter().map(|(procedure, requested_fields)| {
        execute_procedure(
            config,
            &mutation_request,
            database.clone(),
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
    config: &'a MongoConfiguration,
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
                let native_mutation = config.native_mutations().get(name);
                let procedure = native_mutation
                    .ok_or(name.to_string())
                    .map(|native_mutation| {
                        Procedure::from_native_mutation(native_mutation, arguments.clone())
                    })?;
                Ok((procedure, fields.as_ref()))
            }
        })
        .partition_result();

    if !not_found.is_empty() {
        return Err(MutationError::UnprocessableContent(error_response(
            format!(
                "request includes unknown mutations: {}",
                not_found.join(", ")
            ),
        )));
    }

    Ok(procedures)
}

async fn execute_procedure(
    config: &MongoConfiguration,
    mutation_request: &MutationRequest,
    database: Database,
    procedure: Procedure<'_>,
    requested_fields: Option<&NestedField>,
) -> Result<MutationOperationResults, MutationError> {
    let (result, result_type) = procedure
        .execute(database.clone())
        .await
        .map_err(|err| MutationError::UnprocessableContent(error_response(err.to_string())))?;

    let rewritten_result = rewrite_response(requested_fields, result.into())?;

    let requested_result_type = if let Some(fields) = requested_fields {
        let plan_field = type_annotated_nested_field(
            config,
            &mutation_request.collection_relationships,
            &result_type,
            fields.clone(),
        )
        .map_err(|err| MutationError::UnprocessableContent(error_response(err.to_string())))?;
        type_for_nested_field(&[], &result_type, &plan_field)
            .map_err(|err| MutationError::UnprocessableContent(error_response(err.to_string())))?
    } else {
        result_type
    };

    let json_result = bson_to_json(
        config.extended_json_mode(),
        &requested_result_type,
        rewritten_result,
    )
    .map_err(|err| MutationError::UnprocessableContent(error_response(err.to_string())))?;

    Ok(MutationOperationResults::Procedure {
        result: json_result,
    })
}

/// We need to traverse requested fields to rename any fields that are aliased in the GraphQL
/// request
fn rewrite_response(
    requested_fields: Option<&NestedField>,
    value: Bson,
) -> Result<Bson, MutationError> {
    match (requested_fields, value) {
        (None, value) => Ok(value),

        (Some(NestedField::Object(fields)), Bson::Document(doc)) => {
            Ok(rewrite_doc(fields, doc)?.into())
        }
        (Some(NestedField::Array(fields)), Bson::Array(values)) => {
            Ok(rewrite_array(fields, values)?.into())
        }

        (Some(NestedField::Object(_)), _) => Err(MutationError::UnprocessableContent(
            error_response("expected an object".to_owned()),
        )),
        (Some(NestedField::Array(_)), _) => Err(MutationError::UnprocessableContent(
            error_response("expected an array".to_owned()),
        )),
    }
}

fn rewrite_doc(
    fields: &NestedObject,
    mut doc: bson::Document,
) -> Result<bson::Document, MutationError> {
    fields
        .fields
        .iter()
        .map(|(name, field)| {
            let field_value = match field {
                ndc::Field::Column {
                    column,
                    fields,
                    arguments: _,
                } => {
                    let orig_value = doc.remove(column.as_str()).ok_or_else(|| {
                        MutationError::UnprocessableContent(error_response(format!(
                            "missing expected field from response: {name}"
                        )))
                    })?;
                    rewrite_response(fields.as_ref(), orig_value)
                }
                ndc::Field::Relationship { .. } => Err(MutationError::UnsupportedOperation(
                    error_response("The MongoDB connector does not support relationship references in mutations"
                        .to_owned()),
                )),
            }?;

            Ok((name.to_string(), field_value))
        })
        .try_collect()
}

fn rewrite_array(fields: &ndc::NestedArray, values: Vec<Bson>) -> Result<Vec<Bson>, MutationError> {
    let nested = &fields.fields;
    values
        .into_iter()
        .map(|value| rewrite_response(Some(nested), value))
        .try_collect()
}
