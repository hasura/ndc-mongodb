use insta::assert_yaml_snapshot;

use crate::graphql_query;

#[tokio::test]
async fn filters_on_extended_json_using_string_comparison() -> anyhow::Result<()> {
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
              testCases_nestedCollection(where: { staff: { name: { _eq: "Freeman" } } }) {
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
