use configuration::native_queries::NativeQuery;
use dc_api::JsonResponse;
use dc_api_types::{QueryResponse, ResponseFieldValue, RowSet};
use mongodb::Database;

use crate::interface_types::MongoAgentError;

pub async fn handle_native_query_request(
    native_query: NativeQuery,
    database: Database,
) -> Result<JsonResponse<QueryResponse>, MongoAgentError> {
    let result = database
        .run_command(native_query.command, native_query.selection_criteria)
        .await?;
    let result_json =
        serde_json::to_value(result).map_err(|err| MongoAgentError::AdHoc(err.into()))?;

    // A function returs a single row with a single column called `__value`
    // https://hasura.github.io/ndc-spec/specification/queries/functions.html
    let response_row = [(
        "__value".to_owned(),
        ResponseFieldValue::Column(result_json),
    )]
    .into_iter()
    .collect();

    Ok(JsonResponse::Value(QueryResponse::Single(RowSet {
        aggregates: None,
        rows: Some(vec![response_row]),
    })))
}
