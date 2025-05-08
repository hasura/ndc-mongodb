use futures::Stream;
use futures_util::TryStreamExt as _;
use mongodb::bson;
use mongodb_support::aggregate::Pipeline;
use ndc_models::{QueryRequest, QueryResponse};
use ndc_query_plan::plan_for_query_request;
use tracing::{instrument, Instrument};

use super::{pipeline::pipeline_for_query_request, response::serialize_query_response};
use crate::{
    interface_types::MongoAgentError,
    mongo_query_plan::{MongoConfiguration, QueryPlan},
    mongodb::{CollectionTrait as _, DatabaseTrait},
    query::QueryTarget,
};

type Result<T> = std::result::Result<T, MongoAgentError>;

/// Execute a query request against the given collection.
///
/// The use of `DatabaseTrait` lets us inject a mock implementation of the MongoDB driver for
/// testing.
pub async fn execute_query_request(
    database: impl DatabaseTrait,
    config: &MongoConfiguration,
    query_request: QueryRequest,
) -> Result<QueryResponse> {
    tracing::debug!(
        query_request = %serde_json::to_string(&query_request).unwrap(),
        "query request"
    );
    let query_plan = preprocess_query_request(config, query_request)?;
    tracing::debug!(?query_plan, "abstract query plan");
    let pipeline = pipeline_for_query_request(config, &query_plan)?;
    let documents = execute_query_pipeline(database, config, &query_plan, pipeline).await?;
    let response =
        serialize_query_response(config.serialization_options(), &query_plan, documents)?;
    Ok(response)
}

#[instrument(name = "Pre-process Query Request", skip_all, fields(internal.visibility = "user"))]
fn preprocess_query_request(
    config: &MongoConfiguration,
    query_request: QueryRequest,
) -> Result<QueryPlan> {
    let query_plan = plan_for_query_request(config, query_request)?;
    Ok(query_plan)
}

#[instrument(name = "Execute Query Pipeline", skip_all, fields(internal.visibility = "user"))]
async fn execute_query_pipeline(
    database: impl DatabaseTrait,
    config: &MongoConfiguration,
    query_plan: &QueryPlan,
    pipeline: Pipeline,
) -> Result<Vec<bson::Document>> {
    let target = QueryTarget::for_request(config, query_plan);
    tracing::info!(
        ?target,
        pipeline = %serde_json::to_string(&pipeline).unwrap(),
        "executing query"
    );

    // The target of a query request might be a collection, or it might be a native query. In the
    // latter case there is no collection to perform the aggregation against. So instead of sending
    // the MongoDB API call `db.<collection>.aggregate` we instead call `db.aggregate`.
    //
    // If the query request includes variable sets then instead of specifying the target collection
    // up front that is deferred until the `$lookup` stage of the aggregation pipeline. That is
    // another case where we call `db.aggregate` instead of `db.<collection>.aggregate`.
    let documents = match (target.input_collection(), query_plan.has_variables()) {
        (Some(collection_name), false) => {
            let collection = database.collection(collection_name.as_str());
            collect_response_documents(
                collection
                    .aggregate(pipeline, None)
                    .instrument(tracing::info_span!(
                        "MongoDB Aggregate Command",
                        internal.visibility = "user"
                    ))
                    .await?,
            )
            .await
        }
        _ => {
            collect_response_documents(
                database
                    .aggregate(pipeline, None)
                    .instrument(tracing::info_span!(
                        "MongoDB Aggregate Command",
                        internal.visibility = "user"
                    ))
                    .await?,
            )
            .await
        }
    }?;
    tracing::debug!(response_documents = %serde_json::to_string(&documents).unwrap(), "response from MongoDB");
    Ok(documents)
}

#[instrument(name = "Collect Response Documents", skip_all, fields(internal.visibility = "user"))]
async fn collect_response_documents(
    document_cursor: impl Stream<Item = std::result::Result<bson::Document, mongodb::error::Error>>,
) -> Result<Vec<bson::Document>> {
    document_cursor
        .into_stream()
        .map_err(MongoAgentError::MongoDB)
        .try_collect::<Vec<_>>()
        .instrument(tracing::info_span!(
            "Collect Pipeline",
            internal.visibility = "user"
        ))
        .await
}
