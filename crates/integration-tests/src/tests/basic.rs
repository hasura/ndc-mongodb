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
async fn sorts_string_column_value_by_date() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query UpdatedAfter($dateInput: Date) {
                  movies(
                    limit: 10,
                    order_by: {id: Asc},
                    where: {lastupdated: {_gt: $dateInput}}
                  ) {
                    title
                    lastupdated
                  }
                }
            "#
        )
        .variables(json!({ "dateInput": "2016-01-01T00:00Z" }))
        .run()
        .await?
    );
    Ok(())
}
