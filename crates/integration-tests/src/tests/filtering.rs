use insta::assert_yaml_snapshot;
use serde_json::json;

use crate::graphql_query;

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
        .variables(json!({ "types": "double|null" }))
        .run()
        .await?
    );
    Ok(())
}
