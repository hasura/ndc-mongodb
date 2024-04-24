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

#[tokio::test]
async fn runs_native_query_with_collection_representation() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        query(
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
