use crate::{graphql_query, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{asc, binop, field, query, query_request, target, variable};
use serde_json::json;

#[tokio::test]
async fn provides_source_and_target_for_remote_relationship() -> anyhow::Result<()> {
    // Skip this test in MongoDB 5 because the example fails there. We're getting an error:
    //
    // > Kind: Command failed: Error code 5491300 (Location5491300): $documents' is not allowed in user requests, labels: {}
    //
    // This means that remote joins are not working in MongoDB 5
    if let Ok(image) = std::env::var("MONGODB_IMAGE") {
        if image == "mongo:5" {
            return Ok(());
        }
    }

    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query AlbumMovies($limit: Int, $movies_limit: Int) {
                  album(limit: $limit, order_by: { title: Asc }) {
                    title
                    movies(limit: $movies_limit, order_by: { title: Asc }) {
                      title
                      runtime
                    }
                    albumId
                  }
                }
            "#
        )
        .variables(json!({ "limit": 11, "movies_limit": 2 }))
        .run()
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn handles_request_with_single_variable_set() -> anyhow::Result<()> {
    // Skip this test in MongoDB 5 because the example fails there. We're getting an error:
    //
    // > Kind: Command failed: Error code 5491300 (Location5491300): $documents' is not allowed in user requests, labels: {}
    //
    // This means that remote joins are not working in MongoDB 5
    if let Ok(image) = std::env::var("MONGODB_IMAGE") {
        if image == "mongo:5" {
            return Ok(());
        }
    }

    assert_yaml_snapshot!(
        run_connector_query(
            query_request()
                .collection("movies")
                .variables([[("id", json!("573a1390f29313caabcd50e5"))]])
                .query(
                    query()
                        .predicate(binop("_eq", target!("_id"), variable!(id)))
                        .fields([field!("title")]),
                ),
        )
        .await?
    );
    Ok(())
}

// #[tokio::test]
// async fn variable_type_does_not_match_column_type() -> anyhow::Result<()> {
//     // Skip this test in MongoDB 5 because the example fails there. We're getting an error:
//     //
//     // > Kind: Command failed: Error code 5491300 (Location5491300): $documents' is not allowed in user requests, labels: {}
//     //
//     // This means that remote joins are not working in MongoDB 5
//     if let Ok(image) = std::env::var("MONGODB_IMAGE") {
//         if image == "mongo:5" {
//             return Ok(());
//         }
//     }
//
//     assert_yaml_snapshot!(
//         run_connector_query(
//             query_request()
//                 .variables([[("count", 1)], [("count", 2)], [("count", 3)]])
//                 .collection("title_word_frequency")
//                 .query(
//                     query()
//                         .predicate(binop("_eq", target!("count"), variable!(count)))
//                         .order_by([asc!("_id")])
//                         .limit(20)
//                         .fields([field!("_id"), field!("count")]),
//                 )
//         )
//         .await?
//     );
//     Ok(())
// }
