use crate::query;
use insta::assert_yaml_snapshot;

#[tokio::test]
async fn runs_native_query_with_function_representation() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        query(
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
