use insta::assert_yaml_snapshot;
use ndc_test_helpers::{
    array_contains, binop, field, is_empty, query, query_request, target, value, variable,
};
use serde_json::json;

use crate::{connector::Connector, graphql_query, run_connector_query};

#[tokio::test]
async fn filters_using_in_operator() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              movies(
                where: { rated: { _in: ["G", "TV-G"] } }
                order_by: { id: Asc }
                limit: 5
              ) {
                title
                rated
              }
            }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_on_extended_json_using_string_comparison() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query Filtering {
                  extendedJsonTestData(where: { value: { _regex: "hello" } }) {
                    type
                    value
                  }
                }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_by_comparisons_on_elements_of_array_field() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              nestedCollection(
                where: { staff: { name: { _eq: "Freeman" } } }
                order_by: { institution: Asc }
              ) {
                institution
              }
            }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_by_comparison_with_a_variable() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .variables([[("title", "The Blue Bird")]])
                .collection("movies")
                .query(
                    query()
                        .predicate(binop("_eq", target!("title"), variable!(title)))
                        .fields([field!("title")]),
                )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_by_array_comparison_contains() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    .predicate(array_contains(target!("cast"), value!("Albert Austin")))
                    .fields([field!("title"), field!("cast")]),
            )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_by_array_comparison_is_empty() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    .predicate(is_empty(target!("writers")))
                    .fields([field!("writers")])
                    .limit(1),
            )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_by_uuid() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request().collection("uuids").query(
                query()
                    .predicate(binop(
                        "_eq",
                        target!("uuid"),
                        value!("40a693d0-c00a-425d-af5c-535e37fdfe9c")
                    ))
                    .fields([field!("name"), field!("uuid"), field!("uuid_as_string")]),
            )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn filters_by_multiple_criteria_using_variable() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query TestQuery($filter: [MoviesOrderBy!] = {}) {
              movies(
                order_by: $filter
                where: { year: { _gt: 0 }, imdb: { rating: { _gt: 9 } } }
                limit: 15
              ) {
                title
                year
                rated
              }
            }
            "#
        )
        .variables(json!({
            "filter": [
                { "year": "Desc" },
                { "rated": "Desc" },
            ]
        }))
        .run()
        .await?
    );
    Ok(())
}
