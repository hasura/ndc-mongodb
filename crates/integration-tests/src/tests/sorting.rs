use insta::assert_yaml_snapshot;

use crate::graphql_query;

#[tokio::test]
async fn sorts_on_extended_json() -> anyhow::Result<()> {
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
                query Sorting {
                  extendedJsonTestData(order_by: { value: Desc }) {
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
async fn sorts_on_nested_field_names_that_require_escaping() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              testCases_weirdFieldNames(limit: 1, order_by: { invalidName: Asc }) {
                invalidName
                invalidObjectName {
                  validName
                }
                validObjectName {
                  invalidNestedName
                }
              }
            }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}
