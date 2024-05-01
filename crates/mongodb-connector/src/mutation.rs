use std::collections::BTreeMap;

use configuration::Configuration;
use futures::future::try_join_all;
use itertools::Itertools;
use mongodb::{
    bson::{self, Bson},
    Database,
};
use mongodb_agent_common::{
    mutation::Mutation, query::serialization::bson_to_json, state::ConnectorState,
};
use ndc_sdk::{
    connector::MutationError,
    json_response::JsonResponse,
    models::{
        Field, MutationOperation, MutationOperationResults, MutationRequest, MutationResponse,
        NestedArray, NestedField, NestedObject, Relationship,
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
    let jobs = look_up_mutations(config, &mutation_request)?;
    let operation_results = try_join_all(jobs.into_iter().map(|(mutation, requested_fields)| {
        execute_mutation(
            &query_context,
            database.clone(),
            &mutation_request.collection_relationships,
            mutation,
            requested_fields,
        )
    }))
    .await?;
    Ok(JsonResponse::Value(MutationResponse { operation_results }))
}

/// Looks up mutations according to the names given in the mutation request, and pairs them with
/// arguments and requested fields. Returns an error if any mutations cannot be found.
fn look_up_mutations<'a, 'b>(
    config: &'a Configuration,
    mutation_request: &'b MutationRequest,
) -> Result<Vec<(Mutation<'a>, Option<&'b NestedField>)>, MutationError> {
    let (mutations, not_found): (Vec<_>, Vec<String>) = mutation_request
        .operations
        .iter()
        .map(|operation| match operation {
            MutationOperation::Procedure {
                name,
                arguments,
                fields,
            } => {
                let native_mutation = config.native_mutations.get(name);
                let mutation = native_mutation.ok_or(name).map(|native_mutation| {
                    Mutation::from_native_mutation(native_mutation, arguments.clone())
                })?;
                Ok((mutation, fields.as_ref()))
            }
        })
        .partition_result();

    if !not_found.is_empty() {
        return Err(MutationError::UnprocessableContent(format!(
            "request includes unknown mutations: {}",
            not_found.join(", ")
        )));
    }

    Ok(mutations)
}

async fn execute_mutation(
    query_context: &QueryContext<'_>,
    database: Database,
    relationships: &BTreeMap<String, Relationship>,
    mutation: Mutation<'_>,
    requested_fields: Option<&NestedField>,
) -> Result<MutationOperationResults, MutationError> {
    let (result, result_type) = mutation
        .execute(&query_context.object_types, database.clone())
        .await
        .map_err(|err| MutationError::UnprocessableContent(err.to_string()))?;

    let rewritten_result = rewrite_response(requested_fields, result.into())?;

    let (requested_result_type, temp_object_types) = prune_type_to_field_selection(
        query_context,
        relationships,
        &[],
        &result_type,
        requested_fields,
    )
    .map_err(|err| MutationError::Other(Box::new(err)))?;
    let object_types = extend_configured_object_types(query_context, temp_object_types);

    let json_result = bson_to_json(&requested_result_type, &object_types, rewritten_result)
        .map_err(|err| MutationError::UnprocessableContent(err.to_string()))?;

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
            "expected an object".to_owned(),
        )),
        (Some(NestedField::Array(_)), _) => Err(MutationError::UnprocessableContent(
            "expected an array".to_owned(),
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
                Field::Column { column, fields } => {
                    let orig_value = doc.remove(column).ok_or_else(|| {
                        MutationError::UnprocessableContent(format!(
                            "missing expected field from response: {name}"
                        ))
                    })?;
                    rewrite_response(fields.as_ref(), orig_value)
                }
                Field::Relationship { .. } => Err(MutationError::UnsupportedOperation(
                    "The MongoDB connector does not support relationship references in mutations"
                        .to_owned(),
                )),
            }?;

            Ok((name.clone(), field_value))
        })
        .try_collect()
}

fn rewrite_array(fields: &NestedArray, values: Vec<Bson>) -> Result<Vec<Bson>, MutationError> {
    let nested = &fields.fields;
    values
        .into_iter()
        .map(|value| rewrite_response(Some(nested), value))
        .try_collect()
}
