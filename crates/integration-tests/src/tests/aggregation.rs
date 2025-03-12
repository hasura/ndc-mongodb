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
                      avg
                      max
                      min
                      sum
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
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query ($types: String!) {
                  extendedJsonTestDataAggregate(
                    filter_input: { where: { type: { _regex: $types } } }
                  ) {
                    value {
                      avg
                      _count
                      max
                      min
                      sum
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
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query ($types: String!) {
                  extendedJsonTestDataAggregate(
                    filter_input: { where: { type: { _regex: $types } } }
                  ) {
                    value {
                      avg
                      _count
                      max
                      min
                      sum
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

#[tokio::test]
async fn returns_null_when_aggregating_empty_result_set() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              moviesAggregate(filter_input: {where: {title: {_eq: "no such movie"}}}) {
                runtime {
                  avg
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

#[tokio::test]
async fn returns_zero_when_counting_empty_result_set() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              moviesAggregate(filter_input: {where: {title: {_eq: "no such movie"}}}) {
                _count
                title {
                  _count
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

#[tokio::test]
async fn returns_zero_when_counting_nested_fields_in_empty_result_set() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              moviesAggregate(filter_input: {where: {title: {_eq: "no such movie"}}}) {
                awards {
                  nominations {
                    _count
                  }
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

#[tokio::test]
async fn aggregates_nested_field_values() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              moviesAggregate(
                filter_input: {where: {title: {_in: ["Within Our Gates", "The Ace of Hearts"]}}}
              ) {
                tomatoes {
                  viewer {
                    rating {
                      avg
                    }
                  }
                  critic {
                    rating {
                      avg
                    }
                  }
                }
                imdb {
                  rating {
                    avg
                  }
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
