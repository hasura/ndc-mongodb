use configuration::Configuration;
use dc_api_types::QueryRequest;
use futures::Stream;
use futures_util::TryStreamExt as _;
use mongodb::bson;

use super::pipeline::pipeline_for_query_request;
use crate::{
    interface_types::MongoAgentError,
    mongodb::{CollectionTrait as _, DatabaseTrait},
    query::QueryTarget,
};

/// Execute a query request against the given collection.
///
/// The use of `DatabaseTrait` lets us inject a mock implementation of the MongoDB driver for
/// testing.
pub async fn execute_query_request(
    database: impl DatabaseTrait,
    config: &Configuration,
    query_request: QueryRequest,
) -> Result<Vec<bson::Document>, MongoAgentError> {
    let target = QueryTarget::for_request(config, &query_request);
    let pipeline = pipeline_for_query_request(config, &query_request)?;
    tracing::debug!(
        ?query_request,
        ?target,
        pipeline = %serde_json::to_string(&pipeline).unwrap(),
        "executing query"
    );

    // The target of a query request might be a collection, or it might be a native query. In the
    // latter case there is no collection to perform the aggregation against. So instead of sending
    // the MongoDB API call `db.<collection>.aggregate` we instead call `db.aggregate`.
    let documents = match target.input_collection() {
        Some(collection_name) => {
            let collection = database.collection(&collection_name);
            collect_from_cursor(collection.aggregate(pipeline, None).await?).await
        }
        None => collect_from_cursor(database.aggregate(pipeline, None).await?).await,
    }?;
    tracing::debug!(response_documents = %serde_json::to_string(&documents).unwrap(), "response from MongoDB");

    Ok(documents)
}

async fn collect_from_cursor(
    document_cursor: impl Stream<Item = Result<bson::Document, mongodb::error::Error>>,
) -> Result<Vec<bson::Document>, MongoAgentError> {
    document_cursor
        .into_stream()
        .map_err(MongoAgentError::MongoDB)
        .try_collect::<Vec<_>>()
        .await
}
