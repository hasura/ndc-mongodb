use anyhow::anyhow;
use dc_api_types::{QueryRequest, QueryResponse, RowSet};
use futures_util::TryStreamExt;
use itertools::Itertools as _;
use mongodb::bson::{self, Document};

use super::pipeline::{pipeline_for_query_request, ResponseShape};
use crate::{
    interface_types::MongoAgentError, mongodb::CollectionTrait, query::foreach::foreach_variants,
};

pub async fn execute_query_request(
    collection: &impl CollectionTrait<Document>,
    query_request: QueryRequest,
) -> Result<QueryResponse, MongoAgentError> {
    let (pipeline, response_shape) = pipeline_for_query_request(&query_request)?;
    tracing::debug!(
        ?query_request,
        pipeline = %serde_json::to_string(&pipeline).unwrap(),
        "executing query"
    );

    let document_cursor = collection.aggregate(pipeline, None).await?;

    let documents = document_cursor
        .into_stream()
        .map_err(MongoAgentError::MongoDB)
        .try_collect::<Vec<_>>()
        .await?;

    tracing::debug!(response_documents = %serde_json::to_string(&documents).unwrap(), "response from MongoDB");

    let response = match (foreach_variants(&query_request), response_shape) {
        (Some(_), _) => parse_single_document(documents)?,
        (None, ResponseShape::ListOfRows) => QueryResponse::Single(RowSet::Rows {
            rows: documents
                .into_iter()
                .map(bson::from_document)
                .try_collect()?,
        }),
        (None, ResponseShape::SingleObject) => {
            QueryResponse::Single(parse_single_document(documents)?)
        }
    };
    tracing::debug!(response = %serde_json::to_string(&response).unwrap(), "query response");

    Ok(response)
}

fn parse_single_document<T>(documents: Vec<Document>) -> Result<T, MongoAgentError>
where
    T: for<'de> serde::Deserialize<'de>,
{
    let document = documents.into_iter().next().ok_or_else(|| {
        MongoAgentError::AdHoc(anyhow!(
            "Expected a response document from MongoDB, but did not get one"
        ))
    })?;
    let value = bson::from_document(document)?;
    Ok(value)
}
