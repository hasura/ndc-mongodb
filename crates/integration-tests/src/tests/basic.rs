use crate::graphql_query;
use insta::assert_yaml_snapshot;

#[tokio::test]
async fn runs_a_query() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query Movies {
                  movies(limit: 10, order_by: { id: Asc }) {
                    title
                    imdb {
                      rating
                      votes
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
