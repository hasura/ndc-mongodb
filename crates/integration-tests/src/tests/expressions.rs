use insta::assert_yaml_snapshot;

use crate::graphql_query;

// TODO: this duplicates a test in basic
#[tokio::test]
async fn evaluates_field_name_that_requires_escaping_in_nested_expression() -> anyhow::Result<()> {
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
