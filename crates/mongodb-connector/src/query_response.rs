use std::collections::BTreeMap;

use anyhow::anyhow;
use configuration::Configuration;
use indexmap::IndexMap;
use itertools::Itertools as _;
use mongodb::bson::{self, from_bson, Bson};
use ndc_sdk::{
    connector::QueryError,
    models::{Query, QueryRequest, QueryResponse, RowFieldValue, RowSet},
};
use serde::Deserialize;

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
        let rows = serialize_rows(docs)?;
        Ok(RowSet {
            aggregates: None,
            rows: Some(rows),
        })
    } else {
        // When there are aggregates we expect a single document with `rows` and `aggregates`
        // fields
        let row_set: BsonRowSet = parse_single_document(docs)?;
        let aggregates: IndexMap<String, serde_json::Value> = row_set
            .aggregates
            .into_iter()
            .map(|(key, value)| {
                Ok((
                    key,
                    from_bson(value).map_err(|err| QueryError::Other(err.into()))?,
                ))
            })
            .try_collect::<_, _, QueryError>()?;
        let rows = serialize_rows(row_set.rows)?;
        Ok(RowSet {
            aggregates: if aggregates.is_empty() {
                None
            } else {
                Some(aggregates)
            },
            rows: if rows.is_empty() { None } else { Some(rows) },
        })
    }
}

fn serialize_rows(
    docs: Vec<bson::Document>,
) -> Result<Vec<IndexMap<String, RowFieldValue>>, QueryError> {
    docs.into_iter()
        .map(|doc| bson::from_document(doc))
        .try_collect()
        .map_err(|err| QueryError::Other(err.into()))
}

fn parse_single_document<T>(documents: Vec<bson::Document>) -> Result<T, QueryError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let document = documents.into_iter().next().ok_or_else(|| {
        QueryError::Other(
            (anyhow!("expected a single response document from MongoDB, but did not get one"))
                .into(),
        )
    })?;
    let value = bson::from_document(document).map_err(|err| QueryError::Other(err.into()))?;
    Ok(value)
}
