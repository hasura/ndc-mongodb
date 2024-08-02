use crate::{connector::Connector, graphql_query, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{and, asc, binop, field, query, query_request, target, variable};
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
            Connector::SampleMflix,
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

#[tokio::test]
async fn variable_used_in_multiple_type_contexts() -> anyhow::Result<()> {
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
            Connector::SampleMflix,
            query_request()
                .variables([[("dateInput", "2015-09-15T00:00Z")]])
                .collection("movies")
                .query(
                    query()
                        .predicate(and([
                            binop("_gt", target!("released"), variable!(dateInput)), // type is date
                            binop("_gt", target!("lastupdated"), variable!(dateInput)), // type is string
                        ]))
                        .order_by([asc!("_id")])
                        .limit(20)
                        .fields([
                            field!("_id"),
                            field!("title"),
                            field!("released"),
                            field!("lastupdated")
                        ]),
                )
        )
        .await?
    );
    Ok(())
}
