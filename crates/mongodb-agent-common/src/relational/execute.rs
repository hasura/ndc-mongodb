//! Execution of relational queries against MongoDB.

use futures_util::{Stream, StreamExt, TryStreamExt};
use mongodb::bson::Document;
use ndc_models::{RelationalQuery, RelationalQueryResponse};
use tracing::Instrument;

use crate::{
    interface_types::MongoAgentError,
    mongodb::{CollectionTrait, DatabaseTrait},
    state::ConnectorState,
};

use super::{build_relational_pipeline, ColumnMapping, RelationalError};

/// Execute a relational query and return all rows.
pub async fn execute_relational_query(
    state: &ConnectorState,
    query: RelationalQuery,
) -> Result<RelationalQueryResponse, MongoAgentError> {
    tracing::debug!(
        relational_query = %serde_json::to_string(&query).unwrap_or_else(|_| "<serialization error>".to_string()),
        "relational query request"
    );

    let database = state.database();
    execute_relational_query_impl(database, query).await
}

/// Internal implementation that accepts a database trait for testing.
async fn execute_relational_query_impl(
    database: impl DatabaseTrait,
    query: RelationalQuery,
) -> Result<RelationalQueryResponse, MongoAgentError> {
    let pipeline_result =
        build_relational_pipeline(&query.root_relation).map_err(relational_error)?;

    tracing::debug!(
        collection = %pipeline_result.collection,
        pipeline = %serde_json::to_string(&pipeline_result.pipeline).unwrap_or_else(|_| "<serialization error>".to_string()),
        output_columns = ?pipeline_result.output_columns,
        "executing relational query pipeline"
    );

    let collection = database.collection(&pipeline_result.collection);

    let cursor = collection
        .aggregate(pipeline_result.pipeline, None)
        .instrument(tracing::info_span!(
            "MongoDB Aggregate Command (Relational)",
            internal.visibility = "user"
        ))
        .await
        .map_err(MongoAgentError::MongoDB)?;

    let rows = cursor_to_rows(cursor, &pipeline_result.output_columns).await?;

    tracing::debug!(row_count = rows.len(), "relational query completed");

    Ok(RelationalQueryResponse { rows })
}

/// Execute a relational query and return a stream of rows.
pub async fn execute_relational_query_stream(
    state: &ConnectorState,
    query: RelationalQuery,
) -> Result<impl Stream<Item = Result<Vec<serde_json::Value>, MongoAgentError>>, MongoAgentError> {
    tracing::debug!(
        relational_query = %serde_json::to_string(&query).unwrap_or_else(|_| "<serialization error>".to_string()),
        "relational query stream request"
    );

    let database = state.database();
    execute_relational_query_stream_impl(database, query).await
}

/// Internal implementation that accepts a database trait for testing.
async fn execute_relational_query_stream_impl(
    database: impl DatabaseTrait,
    query: RelationalQuery,
) -> Result<impl Stream<Item = Result<Vec<serde_json::Value>, MongoAgentError>>, MongoAgentError> {
    let pipeline_result =
        build_relational_pipeline(&query.root_relation).map_err(relational_error)?;

    tracing::debug!(
        collection = %pipeline_result.collection,
        pipeline = %serde_json::to_string(&pipeline_result.pipeline).unwrap_or_else(|_| "<serialization error>".to_string()),
        output_columns = ?pipeline_result.output_columns,
        "executing relational query stream pipeline"
    );

    let collection = database.collection(&pipeline_result.collection);

    let cursor = collection
        .aggregate(pipeline_result.pipeline, None)
        .instrument(tracing::info_span!(
            "MongoDB Aggregate Command (Relational Stream)",
            internal.visibility = "user"
        ))
        .await
        .map_err(MongoAgentError::MongoDB)?;

    let output_columns = pipeline_result.output_columns;

    tracing::debug!("relational query stream started");

    let stream = cursor.map(move |doc_result| {
        doc_result
            .map_err(MongoAgentError::MongoDB)
            .map(|doc| document_to_row(&doc, &output_columns))
    });

    Ok(stream)
}

/// Convert a cursor of documents to a vector of rows.
async fn cursor_to_rows(
    cursor: impl Stream<Item = Result<Document, mongodb::error::Error>>,
    output_columns: &ColumnMapping,
) -> Result<Vec<Vec<serde_json::Value>>, MongoAgentError> {
    cursor
        .map(|doc_result| {
            doc_result
                .map_err(MongoAgentError::MongoDB)
                .map(|doc| document_to_row(&doc, output_columns))
        })
        .try_collect()
        .await
}

/// Convert a MongoDB document to a row (array of values in column order).
fn document_to_row(doc: &Document, output_columns: &ColumnMapping) -> Vec<serde_json::Value> {
    output_columns
        .iter()
        .map(|field_name| {
            doc.get(field_name)
                .map(bson_to_json)
                .unwrap_or(serde_json::Value::Null)
        })
        .collect()
}

/// Convert a BSON value to a JSON value for relational queries.
///
/// This function handles all BSON types without relying on Extended JSON format.
/// - Documents and Arrays are stringified as JSON strings (relational mode representation)
/// - Scalar types are converted to their appropriate JSON representations
/// - ObjectId, DateTime, Decimal128, Int64, etc. are converted to strings to avoid
///   Extended JSON format like `{"$oid": "..."}` or `{"$date": "..."}`
fn bson_to_json(bson: &mongodb::bson::Bson) -> serde_json::Value {
    use mongodb::bson::Bson;
    use serde_json::{json, Number, Value};
    use time::{format_description::well_known::Iso8601, OffsetDateTime};

    match bson {
        // Null types
        Bson::Null | Bson::Undefined => Value::Null,

        // Boolean
        Bson::Boolean(b) => Value::Bool(*b),

        // Numbers - Int32 and Double can be represented as JSON numbers
        Bson::Int32(n) => Value::Number((*n).into()),
        Bson::Double(n) => Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),

        // Int64 and Decimal128 are converted to strings to avoid precision loss
        Bson::Int64(n) => Value::String(n.to_string()),
        Bson::Decimal128(n) => Value::String(n.to_string()),

        // String types
        Bson::String(s) => Value::String(s.clone()),
        Bson::Symbol(s) => Value::String(s.clone()),
        Bson::JavaScriptCode(s) => Value::String(s.clone()),

        // ObjectId - convert to hex string (not Extended JSON)
        Bson::ObjectId(oid) => Value::String(oid.to_hex()),

        // DateTime - convert to ISO 8601 string
        Bson::DateTime(date) => {
            let system_time = date.to_system_time();
            let offset_date: OffsetDateTime = system_time.into();
            offset_date
                .format(&Iso8601::DEFAULT)
                .map(Value::String)
                .unwrap_or(Value::Null)
        }

        // Timestamp - use a simple JSON object representation
        Bson::Timestamp(ts) => json!({
            "t": ts.time,
            "i": ts.increment
        }),

        // Binary data - convert to base64 with subtype
        Bson::Binary(binary) => {
            use serde_with::base64::{Base64, Standard};
            use serde_with::formats::Padded;
            use serde_with::SerializeAs;

            // Use serde_with's Base64 serializer
            let base64_str: String = Base64::<Standard, Padded>::serialize_as(
                &binary.bytes,
                serde_json::value::Serializer,
            )
            .unwrap_or_else(|_| Value::String(String::new()))
            .as_str()
            .unwrap_or("")
            .to_string();

            json!({
                "base64": base64_str,
                "subType": format!("{:02x}", u8::from(binary.subtype))
            })
        }

        // Regex - use pattern/options representation
        Bson::RegularExpression(regex) => json!({
            "pattern": regex.pattern,
            "options": regex.options
        }),

        // JavaScript with scope - stringify the scope
        Bson::JavaScriptCodeWithScope(code_with_scope) => {
            let scope_json: Value = Bson::Document(code_with_scope.scope.clone()).into();
            json!({
                "$code": code_with_scope.code,
                "$scope": scope_json.to_string()
            })
        }

        // MinKey/MaxKey - use distinct sentinel representations
        Bson::MinKey => json!({"$minKey": 1}),
        Bson::MaxKey => json!({"$maxKey": 1}),

        // DbPointer - rare type, use Extended JSON as fallback (fallback to extjson)
        Bson::DbPointer(_) => bson.clone().into_relaxed_extjson(),

        // Stringify nested documents as JSON strings, recursively applying bson_to_json
        // to ensure consistent conversion (no Extended JSON for ObjectId, DateTime, etc.)
        Bson::Document(doc) => {
            let json_obj: serde_json::Map<String, Value> = doc
                .iter()
                .map(|(k, v)| (k.clone(), bson_to_json(v)))
                .collect();
            Value::String(Value::Object(json_obj).to_string())
        }

        // Stringify arrays as JSON strings, recursively applying bson_to_json
        Bson::Array(arr) => {
            let json_arr: Vec<Value> = arr.iter().map(bson_to_json).collect();
            Value::String(Value::Array(json_arr).to_string())
        }
    }
}

/// Convert a RelationalError to a MongoAgentError.
fn relational_error(err: RelationalError) -> MongoAgentError {
    MongoAgentError::BadQuery(anyhow::anyhow!(err))
}

#[cfg(test)]
mod tests {
    use super::bson_to_json;
    use mongodb::bson::{doc, oid::ObjectId, Bson};
    use serde_json::json;

    // Fix #12: Nested BSON-to-JSON recursively applies conversion
    #[test]
    fn nested_document_converts_objectid_to_string() {
        // A document containing an ObjectId should NOT produce Extended JSON
        let oid = ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap();
        let bson = Bson::Document(doc! {
            "name": "test",
            "_id": oid,
        });
        let result = bson_to_json(&bson);
        // Result should be a JSON string (documents are stringified)
        let s = result.as_str().expect("Document should be stringified");
        // Should NOT contain "$oid" (Extended JSON format)
        assert!(
            !s.contains("$oid"),
            "Nested ObjectId should not use Extended JSON, got: {}",
            s
        );
        // Should contain the hex string directly
        assert!(
            s.contains("507f1f77bcf86cd799439011"),
            "Should contain ObjectId hex, got: {}",
            s
        );
    }

    #[test]
    fn nested_array_converts_recursively() {
        // An array containing an ObjectId and Int64 should convert properly
        let oid = ObjectId::parse_str("507f1f77bcf86cd799439011").unwrap();
        let bson = Bson::Array(vec![
            Bson::ObjectId(oid),
            Bson::Int64(9999999999999),
            Bson::String("hello".into()),
        ]);
        let result = bson_to_json(&bson);
        let s = result.as_str().expect("Array should be stringified");
        // Should NOT contain "$oid" or "$numberLong"
        assert!(
            !s.contains("$oid"),
            "Nested ObjectId should not use Extended JSON, got: {}",
            s
        );
        assert!(
            !s.contains("$numberLong"),
            "Nested Int64 should not use Extended JSON, got: {}",
            s
        );
    }

    #[test]
    fn nested_document_with_datetime_avoids_extended_json() {
        let dt = mongodb::bson::DateTime::from_millis(1768867200000); // 2026-01-20
        let bson = Bson::Document(doc! {
            "created_at": dt,
            "value": 42,
        });
        let result = bson_to_json(&bson);
        let s = result.as_str().expect("Document should be stringified");
        // Should NOT contain "$date" (Extended JSON format)
        assert!(
            !s.contains("$date"),
            "Nested DateTime should not use Extended JSON, got: {}",
            s
        );
    }

    // Fix #13: MinKey/MaxKey produce distinct JSON representations
    #[test]
    fn minkey_and_maxkey_are_distinct() {
        let minkey_result = bson_to_json(&Bson::MinKey);
        let maxkey_result = bson_to_json(&Bson::MaxKey);

        assert_ne!(
            minkey_result, maxkey_result,
            "MinKey and MaxKey should produce distinct JSON values"
        );
    }

    #[test]
    fn minkey_has_minkey_marker() {
        let result = bson_to_json(&Bson::MinKey);
        assert_eq!(result, json!({"$minKey": 1}));
    }

    #[test]
    fn maxkey_has_maxkey_marker() {
        let result = bson_to_json(&Bson::MaxKey);
        assert_eq!(result, json!({"$maxKey": 1}));
    }
}
