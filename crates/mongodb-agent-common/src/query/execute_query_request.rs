use std::collections::BTreeMap;

use anyhow::anyhow;
use bytes::Bytes;
use configuration::native_query::NativeQuery;
use dc_api::JsonResponse;
use dc_api_types::{QueryRequest, QueryResponse};
use futures_util::{Stream, TryStreamExt as _};
use mongodb::bson::{doc, Document};

use super::pipeline::{pipeline_for_query_request, ResponseShape};
use crate::{
    interface_types::MongoAgentError,
    mongodb::{CollectionTrait, DatabaseTrait},
    query::query_target::QueryTarget,
};

/// Execute a query request against the given collection.
///
/// The use of `DatabaseTrait` lets us inject a mock implementation of the MongoDB driver for
/// testing.
pub async fn execute_query_request(
    database: impl DatabaseTrait,
    query_request: QueryRequest,
    native_queries: &BTreeMap<String, NativeQuery>,
) -> Result<JsonResponse<QueryResponse>, MongoAgentError> {
    let target = QueryTarget::for_request(&query_request, native_queries);

    let (pipeline, response_shape) = pipeline_for_query_request(&query_request, native_queries)?;
    tracing::debug!(pipeline = %serde_json::to_string(&pipeline).unwrap(), target = %target, "aggregate pipeline");

    // The target of a query request might be a collection, or it might be a native query. In the
    // latter case there is no collection to perform the aggregation against. So instead of sending
    // the MongoDB API call `db.<collection>.aggregate` we instead call `db.aggregate`.
    let documents = match target {
        QueryTarget::Collection(collection_name) => {
            let collection = database.collection(&collection_name);
            collect_from_cursor(collection.aggregate(pipeline, None).await?).await
        }
        QueryTarget::NativeQuery { .. } => {
            collect_from_cursor(database.aggregate(pipeline, None).await?).await
        }
    }?;

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

async fn collect_from_cursor(
    document_cursor: impl Stream<Item = Result<Document, mongodb::error::Error>>,
) -> Result<Vec<Document>, MongoAgentError> {
    document_cursor
        .into_stream()
        .map_err(MongoAgentError::MongoDB)
        .try_collect::<Vec<_>>()
        .await
}
