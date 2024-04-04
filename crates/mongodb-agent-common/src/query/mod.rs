pub mod arguments;
mod column_ref;
mod constants;
mod execute_native_query_request;
mod execute_query_request;
mod foreach;
mod make_selector;
mod make_sort;
mod pipeline;
mod relations;
pub mod serialization;

use dc_api::JsonResponse;
use dc_api_types::{QueryRequest, QueryResponse, Target};
use mongodb::bson::Document;

use self::execute_query_request::execute_query_request;
pub use self::{
    make_selector::make_selector,
    make_sort::make_sort,
    pipeline::{is_response_faceted, pipeline_for_non_foreach, pipeline_for_query_request},
};
use crate::{
    interface_types::{MongoAgentError, MongoConfig},
    query::execute_native_query_request::handle_native_query_request,
};

pub fn collection_name(query_request_target: &Target) -> String {
    query_request_target.name().join(".")
}

pub async fn handle_query_request(
    config: &MongoConfig,
    query_request: &QueryRequest,
) -> Result<JsonResponse<QueryResponse>, MongoAgentError> {
    tracing::debug!(?config, query_request = %serde_json::to_string(query_request).unwrap(), "executing query");

    let database = config.client.database(&config.database);

    let target = &query_request.target;
    let target_name = {
        let name = target.name();
        if name.len() == 1 {
            Some(&name[0])
        } else {
            None
        }
    };
    if let Some(native_query) = target_name.and_then(|name| config.native_queries.get(name)) {
        return handle_native_query_request(native_query.clone(), database).await;
    }

    let collection = database.collection::<Document>(&collection_name(&query_request.target));

    execute_query_request(&collection, query_request).await
}

#[cfg(test)]
mod tests {
    use dc_api_types::{QueryRequest, QueryResponse};
    use mongodb::{
        bson::{self, bson, doc, from_document, to_bson},
        options::AggregateOptions,
    };
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json, to_value};

    use super::execute_query_request;
    use crate::mongodb::{test_helpers::mock_stream, MockCollectionTrait};

    #[tokio::test]
    async fn executes_query() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "fields": {
                    "student_gpa": { "type": "column", "column": "gpa", "column_type": "double" },
                },
                "where": {
                    "type": "binary_op",
                    "column": { "name": "gpa", "column_type": "double" },
                    "operator": "less_than",
                    "value": { "type": "scalar", "value": 4.0, "value_type": "double" }
                },
            },
            "target": {"name": ["students"], "type": "table"},
            "relationships": [],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [
                { "student_gpa": 3.1 },
                { "student_gpa": 3.6 },
            ],
        }))?;

        let expected_pipeline = json!([
            { "$match": { "gpa": { "$lt": 4.0 } } },
            { "$replaceWith": { "student_gpa": { "$ifNull": ["$gpa", null] } } },
        ]);

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_value(pipeline).unwrap());
                Ok(mock_stream(vec![
                    Ok(from_document(doc! { "student_gpa": 3.1, })?),
                    Ok(from_document(doc! { "student_gpa": 3.6, })?),
                ]))
            });

        let result = execute_query_request(&collection, query_request)
            .await?
            .into_value()?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "aggregates": {
                    "count": {
                        "type": "column_count",
                        "column": "gpa",
                        "distinct": true,
                    },
                    "avg": {
                        "type": "single_column",
                        "column": "gpa",
                        "function": "avg",
                        "result_type": "double",
                    },
                },
            },
            "target": {"name": ["students"], "type": "table"},
            "relationships": [],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "aggregates": {
                "count": 11,
                "avg": 3,
            }
        }))?;

        let expected_pipeline = json!([
            {
                "$facet": {
                    "avg": [
                        { "$match": { "gpa": { "$exists": true, "$ne": null } } },
                        { "$group": { "_id": null, "result": { "$avg": "$gpa" } } },
                    ],
                    "count": [
                        { "$match": { "gpa": { "$exists": true, "$ne": null } } },
                        { "$group": { "_id": "$gpa" } },
                        { "$count": "result" },
                    ],
                },
            },
            {
                "$replaceWith": {
                    "aggregates": {
                        "avg": { "$getField": {
                            "field": "result",
                            "input": { "$first": { "$getField": { "$literal": "avg" } } },
                        } },
                        "count": { "$getField": {
                            "field": "result",
                            "input": { "$first": { "$getField": { "$literal": "count" } } },
                        } },
                    },
                },
            },
        ]);

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_value(pipeline).unwrap());
                Ok(mock_stream(vec![Ok(from_document(doc! {
                    "aggregates": {
                        "count": 11,
                        "avg": 3,
                    },
                })?)]))
            });

        let result = execute_query_request(&collection, query_request)
            .await?
            .into_value()?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation_with_fields() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
            "query": {
                "aggregates": {
                    "avg": {
                        "type": "single_column",
                        "column": "gpa",
                        "function": "avg",
                        "result_type": "double",
                    },
                },
                "fields": {
                    "student_gpa": { "type": "column", "column": "gpa", "column_type": "double" },
                },
                "where": {
                    "type": "binary_op",
                    "column": { "name": "gpa", "column_type": "double" },
                    "operator": "less_than",
                    "value": { "type": "scalar", "value": 4.0, "value_type": "double" }
                },
            },
            "target": {"name": ["students"], "type": "table"},
            "relationships": [],
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "aggregates": {
                "avg": 3.1,
            },
            "rows": [{
                "gpa": 3.1,
            }],
        }))?;

        let expected_pipeline = json!([
            { "$match": { "gpa": { "$lt": 4.0 } } },
            {
                "$facet": {
                    "avg": [
                        { "$match": { "gpa": { "$exists": true, "$ne": null } } },
                        { "$group": { "_id": null, "result": { "$avg": "$gpa" } } },
                    ],
                    "__ROWS__": [{
                        "$replaceWith": {
                            "student_gpa": { "$ifNull": ["$gpa", null] },
                        },
                    }],
                },
            },
            {
                "$replaceWith": {
                    "aggregates": {
                        "avg": { "$getField": {
                            "field": "result",
                            "input": { "$first": { "$getField": { "$literal": "avg" } } },
                        } },
                    },
                    "rows": "$__ROWS__",
                },
            },
        ]);

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_value(pipeline).unwrap());
                Ok(mock_stream(vec![Ok(from_document(doc! {
                    "aggregates": {
                        "avg": 3.1,
                    },
                    "rows": [{
                        "gpa": 3.1,
                    }],
                })?)]))
            });

        let result = execute_query_request(&collection, query_request)
            .await?
            .into_value()?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn converts_date_inputs_to_bson() -> Result<(), anyhow::Error> {
        let query_request: QueryRequest = from_value(json!({
          "query": {
            "fields": {
              "date": { "type": "column", "column": "date", "column_type": "date", },
            },
            "where": {
              "type": "binary_op",
              "column": { "column_type": "date", "name": "date" },
              "operator": "greater_than_or_equal",
              "value": {
                "type": "scalar",
                "value": "2018-08-14T07:05-0800",
                "value_type": "date"
              }
            }
          },
          "target": { "type": "table", "name": [ "comments" ] },
          "relationships": []
        }))?;

        let expected_response: QueryResponse = from_value(json!({
            "rows": [{
                "date": "2018-08-14T15:05:03.142Z",
            }]
        }))?;

        let expected_pipeline = bson!([
            {
                "$match": {
                    "date": { "$gte": bson::DateTime::builder().year(2018).month(8).day(14).hour(15).minute(5).build().unwrap() },
                }
            },
            {
                "$replaceWith": {
                    "date": {
                        "$dateToString": {
                            "date": { "$ifNull": ["$date", null] },
                        },
                    },
                }
            },
        ]);

        let mut collection = MockCollectionTrait::new();
        collection
            .expect_aggregate()
            .returning(move |pipeline, _: Option<AggregateOptions>| {
                assert_eq!(expected_pipeline, to_bson(&pipeline).unwrap());
                Ok(mock_stream(vec![Ok(from_document(doc! {
                    "date": "2018-08-14T15:05:03.142Z",
                })?)]))
            });

        let result = execute_query_request(&collection, &query_request)
            .await?
            .into_value()?;
        assert_eq!(expected_response, result);
        Ok(())
    }
}
