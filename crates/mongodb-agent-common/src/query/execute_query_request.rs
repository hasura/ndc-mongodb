use anyhow::anyhow;
use dc_api_types::{QueryRequest, QueryResponse};
use futures_util::TryStreamExt;
use mongodb::bson::{self, doc, Document};

use super::pipeline::{pipeline_for_query_request, ResponseShape};
use crate::{interface_types::MongoAgentError, mongodb::CollectionTrait};

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

    let response_document: Document = match response_shape {
        ResponseShape::RowStream => {
            doc! { "rows": documents }
        }
        ResponseShape::SingleObject => documents.into_iter().next().ok_or_else(|| {
            MongoAgentError::AdHoc(anyhow!(
                "Expected a response document from MongoDB, but did not get one"
            ))
        })?,
    };
    tracing::debug!(response_document = %serde_json::to_string(&response_document).unwrap(), "response from MongoDB");

    let response = bson::from_document(response_document)?;
    Ok(response)
}
