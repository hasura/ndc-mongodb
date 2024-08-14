use insta::assert_yaml_snapshot;
use serde_json::json;

use crate::graphql_query;

#[tokio::test]
async fn runs_aggregation_over_top_level_fields() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query($albumId: Int!) {
                  track(order_by: { id: Asc }, where: { albumId: { _eq: $albumId } }) {
                    milliseconds
                    unitPrice
                  }
                  trackAggregate(
                    filter_input: { order_by: { id: Asc }, where: { albumId: { _eq: $albumId } } }
                  ) {
                    _count
                    milliseconds {
                      _avg
                      _max
                      _min
                      _sum
                    }
                    unitPrice {
                      _count
                      _count_distinct
                    }
                  }
                }
            "#
        )
        .variables(json!({ "albumId": 9 }))
        .run()
        .await?
    );
    Ok(())
}
