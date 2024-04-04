use anyhow::anyhow;
use bytes::Bytes;
use dc_api::JsonResponse;
use dc_api_types::{QueryRequest, QueryResponse};
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};

use super::pipeline::{pipeline_for_query_request, ResponseShape};
use crate::{interface_types::MongoAgentError, mongodb::CollectionTrait};

pub async fn execute_query_request(
    collection: &impl CollectionTrait<Document>,
    query_request: QueryRequest,
) -> Result<JsonResponse<QueryResponse>, MongoAgentError> {
    let (pipeline, response_shape) = pipeline_for_query_request(&query_request)?;
    tracing::debug!(pipeline = %serde_json::to_string(&pipeline).unwrap(), "aggregate pipeline");

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

    let bytes: Bytes = serde_json::to_vec(&response_document)
        .map_err(MongoAgentError::Serialization)?
        .into();
    Ok(JsonResponse::Serialized(bytes))
}
