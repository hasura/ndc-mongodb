use insta::assert_yaml_snapshot;
use ndc_test_helpers::{binop, field, query, query_request, target, value, variable};

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
async fn filters_by_comparisons_on_elements_of_array_of_scalars_against_variable(
) -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .variables([[("cast_member", "Albert Austin")]])
                .collection("movies")
                .query(
                    query()
                        .predicate(binop("_eq", target!("cast"), variable!(cast_member)))
                        .fields([field!("title"), field!("cast")]),
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
