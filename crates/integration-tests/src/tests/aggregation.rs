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

#[tokio::test]
async fn aggregates_extended_json_representing_mixture_of_numeric_types() -> anyhow::Result<()> {
    // Skip this test in MongoDB 5 because the example fails there. We're getting an error:
    //
    // > Kind: Command failed: Error code 5491300 (Location5491300): $documents' is not allowed in user requests, labels: {}
    //
    // This doesn't affect native queries that don't use the $documents stage.
    if let Ok(image) = std::env::var("MONGODB_IMAGE") {
        if image == "mongo:5" {
            return Ok(());
        }
    }

    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query ($types: String!) {
                  extendedJsonTestDataAggregate(
                    filter_input: { where: { type: { _regex: $types } } }
                  ) {
                    value {
                      _avg
                      _count
                      _max
                      _min
                      _sum
                      _count_distinct
                    }
                  }
                  extendedJsonTestData(where: { type: { _regex: $types } }) {
                    type
                    value
                  }
                }
            "#
        )
        .variables(json!({ "types": "decimal|double|int|long" }))
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn aggregates_mixture_of_numeric_and_null_values() -> anyhow::Result<()> {
    // Skip this test in MongoDB 5 because the example fails there. We're getting an error:
    //
    // > Kind: Command failed: Error code 5491300 (Location5491300): $documents' is not allowed in user requests, labels: {}
    //
    // This doesn't affect native queries that don't use the $documents stage.
    if let Ok(image) = std::env::var("MONGODB_IMAGE") {
        if image == "mongo:5" {
            return Ok(());
        }
    }

    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query ($types: String!) {
                  extendedJsonTestDataAggregate(
                    filter_input: { where: { type: { _regex: $types } } }
                  ) {
                    value {
                      _avg
                      _count
                      _max
                      _min
                      _sum
                      _count_distinct
                    }
                  }
                  extendedJsonTestData(where: { type: { _regex: $types } }) {
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
