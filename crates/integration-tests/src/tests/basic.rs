use crate::graphql_query;
use insta::assert_yaml_snapshot;
use serde_json::json;

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

#[tokio::test]
async fn filters_by_date() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query ($dateInput: Date) {
                  movies(
                    order_by: {id: Asc},
                    where: {released: {_gt: $dateInput}}
                  ) {
                    title
                    released
                  }
                }
            "#
        )
        .variables(json!({ "dateInput": "2016-03-01T00:00Z" }))
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn selects_array_within_array() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              artistsWithAlbumsAndTracks(limit: 1, order_by: {id: Asc}) {
                name
                albums {
                  title
                  tracks {
                    name
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
async fn selects_field_names_that_require_escaping() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              testCases_weirdFieldNames(limit: 1, order_by: { invalidName: Asc }) {
                invalidName
                invalidObjectName {
                  validName
                }
                validObjectName {
                  invalidNestedName
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
