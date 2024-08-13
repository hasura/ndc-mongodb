use insta::assert_yaml_snapshot;

use crate::graphql_query;

#[tokio::test]
async fn runs_aggregation_over_top_level_fields() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query {
                  track(limit: 3) {
                    unitPrice
                  }
                  trackAggregate(filter_input: {limit: 3}) {
                    _count
                    unitPrice {
                      _count
                      _count_distinct
                      _avg
                      _max
                      _min
                      _sum
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
