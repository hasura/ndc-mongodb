use std::collections::BTreeMap;

use configuration::schema::Type;
use indexmap::IndexMap;
use itertools::Itertools;
use mongodb::bson::{self, from_bson, Bson};
use mongodb_agent_common::query::serialization::{bson_to_json, BsonToJsonError};
use ndc_sdk::models::{
    self as ndc, Aggregate, Field, Query, QueryRequest, QueryResponse, RowFieldValue, RowSet,
};
use serde::Deserialize;
use thiserror::Error;

use crate::api_type_conversions::{ConversionError, QueryContext};

#[derive(Debug, Error)]
pub enum QueryResponseError {
    #[error("{0}")]
    BsonToJson(#[from] BsonToJsonError),

    #[error("{0}")]
    Conversion(#[from] ConversionError),

    #[error("expected a single response document from MongoDB, but did not get one")]
    ExpectedSingleDocument,

    #[error("missing aggregate value in response: {0}")]
    MissingAggregateValue(String),

    #[error("expected {collection_name} to have a field named {column} of type {expected_type:?}, but value is missing from database response")]
    MissingColumnValue {
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
    query_context: &QueryContext<'_>,
    query_request: &QueryRequest,
    response_documents: Vec<bson::Document>,
) -> Result<QueryResponse> {
    tracing::debug!(response_documents = %serde_json::to_string(&response_documents).unwrap(), "response from MongoDB");

    let collection_info = query_context.find_collection(&query_request.collection)?;
    let collection_name = &collection_info.name;

    // If the query request specified variable sets then we should have gotten a single document
    // from MongoDB with fields for multiple sets of results - one for each set of variables.
    let row_sets = if query_request.variables.is_some() {
        let responses: ResponsesForVariableSets = parse_single_document(response_documents)?;
        responses
            .row_sets
            .into_iter()
            .map(|docs| {
                serialize_row_set(query_context, collection_name, &query_request.query, docs)
            })
            .try_collect()
    } else {
        // TODO: in an aggregation response we expect one document instead of a list of documents
        Ok(vec![serialize_row_set(
            query_context,
            collection_name,
            &query_request.query,
            response_documents,
        )?])
    }?;
    let response = QueryResponse(row_sets);
    tracing::debug!(query_response = %serde_json::to_string(&response).unwrap());
    Ok(response)
}

fn serialize_row_set(
    query_context: &QueryContext<'_>,
    collection_name: &str,
    query: &Query,
    docs: Vec<bson::Document>,
) -> Result<RowSet> {
    if query
        .aggregates
        .as_ref()
        .unwrap_or(&IndexMap::new())
        .is_empty()
    {
        // When there are no aggregates we expect a list of rows
        let rows = query
            .fields
            .as_ref()
            .map(|fields| serialize_rows(query_context, collection_name, fields, docs))
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
            .as_ref()
            .map(|aggregates| serialize_aggregates(aggregates, row_set.aggregates))
            .transpose()?;

        let rows = query
            .fields
            .as_ref()
            .map(|fields| serialize_rows(query_context, collection_name, fields, row_set.rows))
            .transpose()?;

        Ok(RowSet { aggregates, rows })
    }
}

fn serialize_aggregates(
    query_aggregates: &IndexMap<String, Aggregate>,
    mut aggregate_values: BTreeMap<String, Bson>,
) -> Result<IndexMap<String, serde_json::Value>> {
    query_aggregates
        .iter()
        .map(
            |(key, aggregate_definition)| match aggregate_values.remove_entry(key) {
                Some((owned_key, value)) => Ok((
                    owned_key,
                    // TODO: bson_to_json
                    from_bson(value).map_err(|_| QueryResponseError::TODORemoveMe)?,
                )),
                None => Err(QueryResponseError::MissingAggregateValue(key.clone())),
            },
        )
        .try_collect()
}

fn serialize_rows(
    query_context: &QueryContext<'_>,
    collection_name: &str,
    query_fields: &IndexMap<String, Field>,
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<String, RowFieldValue>>> {
    docs.into_iter()
        .map(|doc| serialize_single_row(query_context, collection_name, query_fields, doc))
        .try_collect()
}

fn serialize_single_row(
    query_context: &QueryContext<'_>,
    collection_name: &str,
    query_fields: &IndexMap<String, Field>,
    mut doc: bson::Document,
) -> Result<IndexMap<String, RowFieldValue>> {
    query_fields
        .iter()
        .map(|(field_name, field_definition)| {
            let value = serialize_field_value(
                query_context,
                collection_name,
                field_definition,
                field_name,
                &mut doc,
            )?;
            Ok((field_name.clone(), RowFieldValue(value)))
        })
        .try_collect()
}

fn serialize_field_value(
    query_context: &QueryContext<'_>,
    collection_name: &str,
    field_definition: &ndc::Field,
    field_name: &str,
    input: &mut bson::Document,
) -> Result<serde_json::Value> {
    let (bson, field_type) = match field_definition {
        ndc::Field::Column { column, fields } => {
            // TODO: if `field_type` is an object type, build a new object type by filtering down to
            // the filds listed in `fields`
            let field_type = find_field_type(query_context, collection_name, column)?;
            let value = value_from_option(
                collection_name,
                column,
                &field_type,
                input.remove(field_name),
            )?;
            (value, field_type)
        }
        ndc::Field::Relationship {
            query,
            relationship,
            arguments,
        } => todo!(),
    };
    let json = bson_to_json(field_type, &query_context.object_types, bson)?;
    Ok(json)
}

fn find_field_type<'a>(
    query_context: &'a QueryContext<'a>,
    collection_name: &str,
    column: &str,
) -> Result<&'a Type> {
    let object_type = query_context.find_collection_object_type(collection_name)?;
    let field_type = object_type.value.fields.get(column).ok_or_else(|| {
        ConversionError::UnknownObjectTypeField {
            object_type: object_type.name.to_string(),
            field_name: column.to_string(),
        }
    })?;
    Ok(&field_type.r#type)
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
        _ => Err(QueryResponseError::MissingColumnValue {
            collection_name: collection_name.to_string(),
            column: column.to_string(),
            expected_type: expected_type.clone(),
        }),
    }
}
