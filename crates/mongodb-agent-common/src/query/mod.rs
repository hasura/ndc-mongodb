mod aggregates;
pub mod column_ref;
mod execute_query_request;
mod foreach;
mod groups;
mod is_response_faceted;
mod make_selector;
mod make_sort;
mod native_query;
mod pipeline;
mod query_level;
mod query_target;
mod query_variable_name;
mod relations;
pub mod response;
mod selection;
pub mod serialization;

use ndc_models::{QueryRequest, QueryResponse};

use self::execute_query_request::execute_query_request;
pub use self::{
    make_selector::make_selector,
    make_sort::make_sort_stages,
    pipeline::{pipeline_for_non_foreach, pipeline_for_query_request},
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
    use mongodb::bson::{self, bson};
    use ndc_models::{QueryResponse, RowSet};
    use ndc_test_helpers::{
        binop, collection, field, named_type, object_type, query, query_request, row_set, target,
        value,
    };
    use pretty_assertions::assert_eq;

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
                    .predicate(binop("_lt", target!("gpa"), value!(4.0))),
            )
            .into();

        let expected_response = row_set()
            .rows([[("student_gpa", 3.1)], [("student_gpa", 3.6)]])
            .into_response();

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
            .row([("date", "2018-08-14T15:05:00.000000000Z")])
            .into_response();

        let expected_pipeline = bson!([
            {
                "$match": {
                    "date": { "$gte": bson::DateTime::builder().year(2018).month(8).day(14).hour(15).minute(5).build().unwrap() },
                }
            },
            {
                "$replaceWith": {
                    "date": { "$ifNull": ["$date", null] },
                }
            },
        ]);

        let db = mock_collection_aggregate_response_for_pipeline(
            "comments",
            expected_pipeline,
            bson!([{
                "date": bson::DateTime::builder().year(2018).month(8).day(14).hour(15).minute(5).build().unwrap(),
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
            groups: Default::default(),
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
            native_mutations: Default::default(),
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
            native_mutations: Default::default(),
            native_queries: Default::default(),
            options: Default::default(),
        })
    }
}
