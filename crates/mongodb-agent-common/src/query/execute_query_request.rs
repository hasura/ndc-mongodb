use std::collections::BTreeMap;

use anyhow::anyhow;
use configuration::native_query::NativeQuery;
use dc_api_types::{QueryRequest, QueryResponse, RowSet};
use futures::Stream;
use futures_util::TryStreamExt;
use itertools::Itertools as _;
use mongodb::bson::{self, Document};

use super::pipeline::{pipeline_for_query_request, ResponseShape};
use crate::{
    interface_types::MongoAgentError,
    mongodb::{CollectionTrait as _, DatabaseTrait},
    query::{foreach::foreach_variants, QueryTarget},
};

/// Execute a query request against the given collection.
///
/// The use of `DatabaseTrait` lets us inject a mock implementation of the MongoDB driver for
/// testing.
pub async fn execute_query_request(
    database: impl DatabaseTrait,
    query_request: QueryRequest,
    native_queries: &BTreeMap<String, NativeQuery>,
) -> Result<QueryResponse, MongoAgentError> {
    let target = QueryTarget::for_request(&query_request, native_queries);
    let (pipeline, response_shape) = pipeline_for_query_request(&query_request, native_queries)?;
    tracing::debug!(
        ?query_request,
        ?target,
        pipeline = %serde_json::to_string(&pipeline).unwrap(),
        "executing query"
    );

    // The target of a query request might be a collection, or it might be a native query. In the
    // latter case there is no collection to perform the aggregation against. So instead of sending
    // the MongoDB API call `db.<collection>.aggregate` we instead call `db.aggregate`.
    let documents_result = match target {
        QueryTarget::Collection(collection_name) => {
            let collection = database.collection(&collection_name);
            collect_from_cursor(collection.aggregate(pipeline, None).await?).await
        }
        QueryTarget::NativeQuery { .. } => {
            collect_from_cursor(database.aggregate(pipeline, None).await?).await
        }
    };

    let documents = match documents_result {
        Ok(docs) => Ok(docs),
        Err(error) => {
            tracing::warn!(?error, "error response from MongoDB");
            Err(error)
        }
    }?;

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

async fn collect_from_cursor(
    document_cursor: impl Stream<Item = Result<Document, mongodb::error::Error>>,
) -> Result<Vec<Document>, MongoAgentError> {
    document_cursor
        .into_stream()
        .map_err(MongoAgentError::MongoDB)
        .try_collect::<Vec<_>>()
        .await
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
