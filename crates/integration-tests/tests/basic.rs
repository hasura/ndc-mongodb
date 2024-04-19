use insta::assert_yaml_snapshot;
use integration_tests::run_query;

#[tokio::test]
async fn runs_a_query() -> anyhow::Result<()> {
    let query = r#"
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
    let response = run_query(query).await?;
    assert_yaml_snapshot!(response);
    Ok(())
}
