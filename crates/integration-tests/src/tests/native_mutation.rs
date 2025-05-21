use crate::{graphql_query, non_empty_array, GraphQLResponse};
use assert_json::{assert_json, validators};
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

#[tokio::test]
async fn accepts_predicate_argument() -> anyhow::Result<()> {
    let album_id = 3;

    let mutation_resp = graphql_query(
        r#"
            mutation($albumId: Int!) {
              updateTrackPrices(newPrice: "11.99", where: {albumId: {_eq: $albumId}}) {
                n
                ok
              }
            }
        "#,
    )
    .variables(json!({ "albumId": album_id }))
    .run()
    .await?;

    assert_eq!(mutation_resp.errors, None);
    assert_json!(mutation_resp.data, {
        "updateTrackPrices": {
            "ok": 1.0,
            "n": validators::i64(|n| if n > &0 {
                Ok(())
            } else {
                Err("expected number of updated documents to be non-zero".to_string())
            })
        }
    });

    let tracks_resp = graphql_query(
        r#"
            query($albumId: Int!) {
              track(where: {albumId: {_eq: $albumId}}, order_by: {id: Asc}) {
                name
                unitPrice
              }
            }
        "#,
    )
    .variables(json!({ "albumId": album_id }))
    .run()
    .await?;

    assert_json!(tracks_resp.data, {
        "track": non_empty_array().and(validators::array_for_each(validators::object([
            ("unitPrice".to_string(), Box::new(validators::eq("11.99")) as Box<dyn Validator>)
        ].into())))
    });

    Ok(())
}

#[tokio::test]
async fn accepts_inputs_in_extended_json_format() -> anyhow::Result<()> {
    let movie_id = "573a1391f29313caabcd6f98";
    let plot = "A high-stakes game of Crazy Eights takes a dark turn...";
    let released = "1924-04-01T00:00:00.000000000Z";

    let mutation_resp = graphql_query(
        r#"
            mutation($movieId: ObjectId!, $update: ExtendedJson!) {
              updateMovies(movieId: $movieId, update: $update) {
                ok
              }
            }
        "#,
    )
    .variables(json!({
        "movieId": movie_id,
        "update": { "plot": plot, "released": { "$date": released } },
    }))
    .run()
    .await?;

    assert_eq!(mutation_resp.errors, None);
    assert_json!(mutation_resp.data, {
        "updateMovies": {
            "ok": 1.0
        }
    });

    let tracks_resp = graphql_query(
        r#"
            query($movieId: ObjectId!) {
              movies(where: {id: {_eq: $movieId}}) {
                title
                plot
                released
              }
            }
        "#,
    )
    .variables(json!({ "movieId": movie_id }))
    .run()
    .await?;

    assert_eq!(tracks_resp.errors, None);
    assert_json!(tracks_resp.data, {
        "movies": [{
            "title": "The Ace of Hearts",
            "plot": plot,
            "released": released,
        }],
    });

    Ok(())
}
