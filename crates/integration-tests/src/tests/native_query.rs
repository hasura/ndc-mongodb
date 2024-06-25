use crate::graphql_query;
use insta::assert_yaml_snapshot;

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
