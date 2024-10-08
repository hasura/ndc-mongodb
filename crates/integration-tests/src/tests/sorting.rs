use insta::assert_yaml_snapshot;

use crate::graphql_query;

#[tokio::test]
async fn sorts_on_extended_json() -> anyhow::Result<()> {
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
