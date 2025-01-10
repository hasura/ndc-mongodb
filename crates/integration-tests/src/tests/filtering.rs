use insta::assert_yaml_snapshot;
use ndc_test_helpers::{binop, field, query, query_request, target, variable};

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
