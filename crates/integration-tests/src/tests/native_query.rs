use crate::{connector::Connector, graphql_query, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{asc, binop, field, query, query_request, target, variable};

#[tokio::test]
async fn runs_native_query_with_function_representation() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query NativeQuery {
                  hello(name: "world")
                }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn runs_native_query_with_collection_representation() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query {
                  titleWordFrequency(
                    where: {count: {_eq: 2}}
                    order_by: {id: Asc}
                    offset: 100
                    limit: 25
                  ) {
                    id
                    count
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
async fn runs_native_query_with_variable_sets() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .variables([[("count", 1)], [("count", 2)], [("count", 3)]])
                .collection("title_word_frequency")
                .query(
                    query()
                        .predicate(binop("_eq", target!("count"), variable!(count)))
                        .order_by([asc!("_id")])
                        .limit(20)
                        .fields([field!("_id"), field!("count")]),
                )
        )
        .await?
    );
    Ok(())
}
