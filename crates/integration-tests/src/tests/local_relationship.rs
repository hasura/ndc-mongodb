use crate::graphql_query;
use insta::assert_yaml_snapshot;

#[tokio::test]
async fn joins_local_relationships() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query {
                  movies(limit: 2, order_by: {title: Asc}, where: {title: {_iregex: "Rear"}}) {
                    id
                    title
                    comments(limit: 2, order_by: {id: Asc}) {
                      email
                      text
                      movie {
                        id
                        title
                      }
                      user {
                        email
                        comments(limit: 2, order_by: {id: Asc}) {
                          email
                          text
                          user {
                            email
                            comments(limit: 2, order_by: {id: Asc}) {
                              id
                              email
                            }
                          }
                        }
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
async fn filters_by_field_of_related_collection() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              comments(where: {movie: {rated: {_eq: "G"}}}, limit: 10, order_by: {id: Asc}) {
                movie {
                  title
                  year
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
async fn sorts_by_field_of_related_collection() -> anyhow::Result<()> {
    // Filter by rating to filter out comments whose movie relation is null.
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              comments(
                limit: 10
                order_by: [{movie: {title: Asc}}, {date: Asc}]
                where: {movie: {rated: {_eq: "G"}}}
              ) {
                movie {
                  title
                  year
                }
                text
              }
            }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}
