use crate::graphql_query;
use insta::assert_yaml_snapshot;

#[tokio::test]
async fn filters_results_according_to_configured_permissions() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              users(order_by: {id: Asc}) {
                id
                name
                email
                comments(limit: 5, order_by: {id: Asc}) {
                  date
                  email
                  text
                }
              }
              comments(limit: 5, order_by: {id: Asc}) {
                date
                email
                text
              }
            }
            "#
        )
        .headers([
            ("x-hasura-role", "user"),
            ("x-hasura-user-id", "59b99db4cfa9a34dcd7885b6"),
        ])
        .run()
        .await?
    );
    Ok(())
}
