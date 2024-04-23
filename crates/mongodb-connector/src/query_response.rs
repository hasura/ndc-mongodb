use std::collections::{BTreeMap, HashMap};

use anyhow::anyhow;
use configuration::{schema::Type, Configuration};
use dc_api_types::{Aggregate, Field, QueryRequest};
use indexmap::IndexMap;
use itertools::Itertools;
use mongodb::bson::{self, from_bson, Bson};
use mongodb_agent_common::query::{serialization::bson_to_json, QueryTarget};
use mongodb_support::BsonScalarType;
use ndc_sdk::{
    connector::QueryError,
    models::{self as ndc, QueryResponse, RowFieldValue, RowSet},
};
use serde::Deserialize;
use thiserror::Error;

use crate::api_type_conversions::{ConversionError, QueryContext};

#[derive(Clone, Debug, Error)]
pub enum QueryResponseError {
    #[error("{0}")]
    Conversion(#[from] ConversionError),

    #[error("expected a single response document from MongoDB, but did not get one")]
    ExpectedSingleDocument,

    #[error("expected {collection_name} to have a field named {column} of type {expected_type:?}, but value is missing from database response")]
    MissingValue {
        collection_name: String,
        column: String,
        expected_type: Type,
    },

    #[error("placeholder")]
    TODORemoveMe,
}

type Result<T> = std::result::Result<T, QueryResponseError>;

// These structs describe possible shapes of data returned by MongoDB query plans

#[derive(Debug, Deserialize)]
struct ResponsesForVariableSets {
    row_sets: Vec<Vec<bson::Document>>,
}

#[derive(Debug, Deserialize)]
struct BsonRowSet {
    #[serde(default)]
    aggregates: BTreeMap<String, Bson>,
    #[serde(default)]
    rows: Vec<bson::Document>,
}

pub fn serialize_query_response(
    config: &Configuration,
    query_request: &QueryRequest,
    response_documents: Vec<bson::Document>,
) -> Result<QueryResponse, QueryError> {
    tracing::debug!(response_documents = %serde_json::to_string(&response_documents).unwrap(), "response from MongoDB");
    // If the query request specified variable sets then we should have gotten a single document
    // from MongoDB with fields for multiple sets of results - one for each set of variables.
    let row_sets = if query_request.variables.is_some() {
        let responses: ResponsesForVariableSets = parse_single_document(response_documents)?;
        responses
            .row_sets
            .into_iter()
            .map(|docs| serialize_row_set(&query_request.query, docs))
            .try_collect()
    } else {
        // TODO: in an aggregation response we expect one document instead of a list of documents
        Ok(vec![serialize_row_set(
            &query_request.query,
            response_documents,
        )?])
    }?;
    let response = QueryResponse(row_sets);
    tracing::debug!(query_response = %serde_json::to_string(&response).unwrap());
    Ok(response)
}

fn serialize_row_set(query: &Query, docs: Vec<bson::Document>) -> Result<RowSet, QueryError> {
    if query
        .aggregates
        .as_ref()
        .unwrap_or(&IndexMap::new())
        .is_empty()
    {
        // When there are no aggregates we expect a list of rows
        let rows = query
            .fields
            .map(|fields| serialize_rows(fields, docs))
            .transpose()?;
        Ok(RowSet {
            aggregates: None,
            rows,
        })
    } else {
        // When there are aggregates we expect a single document with `rows` and `aggregates`
        // fields
        let row_set: BsonRowSet = parse_single_document(docs)?;

        let aggregates = query
            .aggregates
            .map(|aggregates| serialize_aggregates(aggregates, row_set.aggregates))
            .transpose()?;

        let rows = query
            .fields
            .map(|fields| serialize_rows(fields, row_set.rows))
            .transpose()?;

        Ok(RowSet { aggregates, rows })
    }
}

fn serialize_aggregates(
    query_aggregates: &HashMap<String, Aggregate>,
    mut aggregate_values: BTreeMap<String, Bson>,
) -> Result<IndexMap<String, serde_json::Value>, QueryError> {
    query_aggregates
        .iter()
        .map(
            |(key, aggregate_definition)| match aggregate_values.remove_entry(key) {
                Some((owned_key, value)) => Ok((
                    owned_key,
                    // TODO: bson_to_json
                    from_bson(value).map_err(|err| QueryError::Other(err.into()))?,
                )),
                None => Err(QueryError::Other(
                    anyhow!("missing aggregate value in response: {key}").into(),
                )),
            },
        )
        .try_collect()
}

fn serialize_rows(
    query_target: QueryTarget<'_>,
    query_fields: &IndexMap<String, Field>,
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<String, RowFieldValue>>> {
    docs.into_iter()
        .map(|doc| serialize_single_row(query_fields, doc))
        .try_collect()
        .map_err(|err| QueryError::Other(err.into()))
}

fn serialize_single_row(
    query_context: QueryContext<'_>,
    query_target: QueryTarget<'_>,
    query_fields: &IndexMap<String, Field>,
    mut doc: bson::Document,
) -> Result<IndexMap<String, RowFieldValue>> {
    query_fields
        .iter()
        .map(|(field_name, field_definition)| {
            // let
            let value = doc.remove(field_name);
        })
        .try_collect()

    // doc.into_iter()
    //     .map(|(key, value)| {
    //         let json_value =
    //                 bson_to_json(expected_type, object_types, value)
    //                 // use UnprocessableContent so the user sees the error message
    //                     .map_err(|err|
    //                     QueryError::UnprocessableContent(format!("type mismatch found in MongoDB query response: {}\n\nYou may need to alter your connector configuration to change a collection schema, or a native query definition.", err.to_string())))?;
    //         Ok((
    //             key,
    //             RowFieldValue(
    //                 json_value
    //             ),
    //         ))
    //     })
    //     .try_collect()
}

fn value_and_type_from_field(
    query_context: QueryContext<'_>,
    collection_name: &str,
    field_definition: &ndc::Field,
    field_name: &str,
    input: &mut bson::Document,
) -> Result<(Bson, Type)> {
    match field_definition {
        ndc::Field::Column { column, fields } => {
            let field_type = find_field_type(query_context, collection_name, column)?;
            let value = value_from_option(
                collection_name,
                column,
                &field_type,
                input.remove(field_name),
            )?;
            Ok((value, field_type))
        }
        ndc::Field::Relationship {
            query,
            relationship,
            arguments,
        } => todo!(),
    }
}

fn find_field_type(
    query_context: QueryContext<'_>,
    collection_name: &str,
    column: &str,
) -> Result<Type> {
    let object_type = query_context.find_collection_object_type(collection_name)?;
    let field_type = object_type.value.fields.get(column).ok_or_else(|| {
        ConversionError::UnknownObjectTypeField {
            object_type: object_type.name.to_string(),
            field_name: column.to_string(),
        }
    })?;
    Ok(field_type.r#type)
}

fn parse_single_document<T>(documents: Vec<bson::Document>) -> Result<T>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let document = documents
        .into_iter()
        .next()
        .ok_or(QueryResponseError::ExpectedSingleDocument)?;
    let value = bson::from_document(document).map_err(|_| QueryResponseError::TODORemoveMe)?;
    Ok(value)
}

/// Check option result for a BSON value. If the value is missing but the expected type is nullable
/// then return null. Otherwise return an error.
fn value_from_option(
    collection_name: &str,
    column: &str,
    expected_type: &Type,
    value_option: Option<Bson>,
) -> Result<Bson> {
    match (expected_type, value_option) {
        (_, Some(value)) => Ok(value),
        (Type::Nullable(_), None) => Ok(Bson::Null),
        _ => Err(QueryResponseError::MissingValue {
            collection_name: collection_name.to_string(),
            column: column.to_string(),
            expected_type: expected_type.clone(),
        }),
    }
}
