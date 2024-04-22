use anyhow::anyhow;
use configuration::Configuration;
use itertools::Itertools as _;
use mongodb::bson;
use ndc_sdk::{
    connector::QueryError,
    models::{QueryRequest, QueryResponse, RowSet},
};
use serde::Deserialize;

// These structs describe possible shapes of data returned by MongoDB query plans

#[derive(Debug, Deserialize)]
struct ResponsesForVariableSets {
    row_sets: Vec<Vec<bson::Document>>,
}

// #[derive(Debug, Deserialize)]
// struct ResponseForAVariableSet {
//     query: BsonRowSet,
// }
//
// #[derive(Debug, Deserialize)]
// struct SingleResponse {
//     query: BsonRowSet,
// }

// #[derive(Debug, Deserialize)]
// struct BsonRowSet {
//     rows: Vec<bson::Document>,
// }

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
            .map(|docs| serialize_row_set(docs))
            .try_collect()
    } else {
        // TODO: in an aggregation response we expect one document instead of a list of documents
        Ok(vec![serialize_row_set(response_documents)?])
    }?;
    let response = QueryResponse(row_sets);
    tracing::debug!(query_response = %serde_json::to_string(&response).unwrap());
    Ok(response)
}

fn serialize_row_set(docs: Vec<bson::Document>) -> Result<RowSet, QueryError> {
    let rows = docs
        .into_iter()
        .map(|doc| bson::from_document(doc))
        .try_collect()
        .map_err(|err| QueryError::Other(err.into()))?;
    Ok(RowSet {
        aggregates: None,
        rows: Some(rows),
    })
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
