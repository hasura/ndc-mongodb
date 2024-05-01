use crate::{graphql_query, GraphQLResponse};
use insta::assert_yaml_snapshot;
use serde_json::json;

#[tokio::test]
async fn updates_with_native_mutation() -> anyhow::Result<()> {
    let id_1 = 5471;
    let id_2 = 5472;
    let mutation = r#"
        mutation InsertArtist($id: Int!, $name: String!) {
          insertArtist(id: $id, name: $name) {
            number_of_docs_inserted: n
            ok
          }
        }
    "#;

    let res1 = graphql_query(mutation)
        .variables(json!({ "id": id_1, "name": "Regina Spektor" }))
        .run()
        .await?;
    graphql_query(mutation)
        .variables(json!({ "id": id_2, "name": "Ok Go" }))
        .run()
        .await?;

    assert_eq!(
        res1,
        GraphQLResponse {
            data: json!({
                "insertArtist": {
                    "number_of_docs_inserted": 1,
                    "ok": 1.0,
                }
            }),
            errors: None,
        }
    );

    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query {
                  artist1: artist(where: { artistId: { _eq: 5471 } }, limit: 1) {
                    artistId
                    name
                  }
                  artist2: artist(where: { artistId: { _eq: 5472 } }, limit: 1) {
                    artistId
                    name
                  }
                }
            "#
        )
        .run()
        .await?
    );
    Ok(())
}
