use crate::{graphql_query, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_models::{OrderByElement, OrderByTarget, OrderDirection};
use ndc_test_helpers::{binop, field, query, query_request, target, variable};

#[tokio::test]
async fn runs_native_query_with_function_representation() -> anyhow::Result<()> {
    // Skip this test in MongoDB 5 because the example fails there. We're getting an error:
    //
    // > Kind: Command failed: Error code 5491300 (Location5491300): $documents' is not allowed in user requests, labels: {}
    //
    // This doesn't affect native queries that don't use the $documents stage.
    if let Ok(image) = std::env::var("MONGODB_IMAGE") {
        if image == "mongo:5" {
            return Ok(());
        }
    }

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
                  title_word_frequencies(
                    where: {count: {_eq: 2}}
                    order_by: {word: Asc}
                    offset: 100
                    limit: 25
                  ) {
                    word
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
            query_request()
                .variables([[("count", 1)], [("count", 2)], [("count", 3)]])
                .collection("title_word_frequency")
                .query(
                    query()
                        .predicate(binop("_eq", target!("count"), variable!(count)))
                        .order_by(vec![OrderByElement {
                            order_direction: OrderDirection::Asc,
                            target: OrderByTarget::Column {
                                name: "_id".to_string(),
                                field_path: None,
                                path: vec![],
                            },
                        }])
                        .limit(20)
                        .fields([field!("_id"), field!("count")]),
                )
        )
        .await?
    );
    Ok(())
}
