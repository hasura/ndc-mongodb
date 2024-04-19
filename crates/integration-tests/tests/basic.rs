use insta::assert_yaml_snapshot;
use integration_tests::query;

#[tokio::test]
async fn runs_a_query() -> anyhow::Result<()> {
    let q = r#"
        query Movies {
          movies(limit: 10, order_by: { id: Asc }) {
            title
            imdb {
              rating
              votes
            }
          }
        }
    "#;
    let response = query(q).run().await?;
    assert_yaml_snapshot!(response);
    Ok(())
}
