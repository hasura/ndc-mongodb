pub mod arguments;
mod column_ref;
mod constants;
mod execute_query_request;
mod foreach;
mod make_selector;
mod make_sort;
mod native_query;
mod pipeline;
mod query_target;
mod relations;
pub mod response;
pub mod serialization;

use ndc_models::{QueryRequest, QueryResponse};

use self::execute_query_request::execute_query_request;
pub use self::{
    make_selector::make_selector,
    make_sort::make_sort,
    pipeline::{is_response_faceted, pipeline_for_non_foreach, pipeline_for_query_request},
    query_target::QueryTarget,
    response::QueryResponseError,
};
use crate::{
    interface_types::MongoAgentError, mongo_query_plan::MongoConfiguration, state::ConnectorState,
};

pub async fn handle_query_request(
    config: &MongoConfiguration,
    state: &ConnectorState,
    query_request: QueryRequest,
) -> Result<QueryResponse, MongoAgentError> {
    let database = state.database();
    // This function delegates to another function which gives is a point to inject a mock database
    // implementation for testing.
    execute_query_request(database, config, query_request).await
}

#[cfg(test)]
mod tests {
    use configuration::Configuration;
    use mongodb::bson::{self, bson, doc};
    use ndc_models::{QueryResponse, RowFieldValue, RowSet};
    use ndc_test_helpers::{
        binop, collection, column_aggregate, column_count_aggregate, field, named_type,
        object_type, query, query_request, row_set, target, value,
    };
    use pretty_assertions::assert_eq;
    use serde_json::{from_value, json};

    use super::execute_query_request;
    use crate::{
        mongo_query_plan::MongoConfiguration,
        mongodb::test_helpers::{
            mock_collection_aggregate_response, mock_collection_aggregate_response_for_pipeline,
        },
    };

    #[tokio::test]
    async fn executes_query() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("students")
            .query(
                query()
                    .fields([field!("student_gpa" => "gpa")])
                    .predicate(binop("less_than", target!("gpa"), value!(4.0))),
            )
            .into();

        let expected_response = row_set()
            .rows([("student_gpa", 3.1), ("student_gpa", 3.6)])
            .into();

        let expected_pipeline = bson!([
            { "$match": { "gpa": { "$lt": 4.0 } } },
            { "$replaceWith": { "student_gpa": { "$ifNull": ["$gpa", null] } } },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "students",
            expected_pipeline,
            bson!([
                { "student_gpa": 3.1, },
                { "student_gpa": 3.6, },
            ]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("students")
            .query(query().aggregates([
                column_count_aggregate!("count" => "gpa", distinct: true),
                column_aggregate!("avg" => "gpa", "avg"),
            ]))
            .into();

        let config = MongoConfiguration(Configuration {
            collections: [collection("students")].into(),
            object_types: [(
                "students".into(),
                object_type([("student_gpa", named_type("double"))]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_procedures: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        });

        let expected_response = row_set()
            .aggregates([("count", 11), ("avg", 3)])
            .into_response();

        let expected_pipeline = bson!([
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

        let db = mock_collection_aggregate_response_for_pipeline(
            "students",
            expected_pipeline,
            bson!([{
                "aggregates": {
                    "count": 11,
                    "avg": 3,
                },
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn executes_aggregation_with_fields() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("students")
            .query(
                query()
                    .aggregates([column_aggregate!("avg" => "gpa", "avg")])
                    .fields([field!("student_gpa" => "gpa")])
                    .predicate(binop("_lt", target!("gpa"), value!(4.0))),
            )
            .into();

        let expected_response = row_set()
            .aggregates([("avg", 3.1), ("gpa", 3.1)])
            .rows([("gpa", 3.1)])
            .into();

        let expected_pipeline = bson!([
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

        let db = mock_collection_aggregate_response_for_pipeline(
            "students",
            expected_pipeline,
            bson!([{
                "aggregates": {
                    "avg": 3.1,
                },
                "rows": [{
                    "gpa": 3.1,
                }],
            }]),
        );

        let result = execute_query_request(db, &students_config(), query_request).await?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn converts_date_inputs_to_bson() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("comments")
            .query(query().fields([field!("date")]).predicate(binop(
                "_gte",
                target!("date"),
                value!("2018-08-14T07:05-0800"),
            )))
            .into();

        let expected_response = row_set()
            .rows([("date", "2018-08-14T15:05:03.142Z")])
            .into();

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

        let db = mock_collection_aggregate_response_for_pipeline(
            "comments",
            expected_pipeline,
            bson!([{
                "date": "2018-08-14T15:05:03.142Z",
            }]),
        );

        let result = execute_query_request(db, &comments_config(), query_request).await?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    #[tokio::test]
    async fn parses_empty_response() -> Result<(), anyhow::Error> {
        let query_request = query_request()
            .collection("comments")
            .query(query().fields([field!("date")]))
            .into();

        let expected_response = QueryResponse(vec![RowSet {
            aggregates: None,
            rows: Some(vec![]),
        }]);

        let db = mock_collection_aggregate_response("comments", bson!([]));

        let result = execute_query_request(db, &comments_config(), query_request).await?;
        assert_eq!(expected_response, result);
        Ok(())
    }

    fn students_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("students")].into(),
            object_types: [(
                "students".into(),
                object_type([("gpa", named_type("Double"))]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_procedures: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }

    fn comments_config() -> MongoConfiguration {
        MongoConfiguration(Configuration {
            collections: [collection("comments")].into(),
            object_types: [(
                "comments".into(),
                object_type([("date", named_type("Date"))]),
            )]
            .into(),
            functions: Default::default(),
            procedures: Default::default(),
            native_procedures: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }
}
