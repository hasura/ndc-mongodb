use insta::assert_yaml_snapshot;
use ndc_models::ExistsInCollection;
use ndc_test_helpers::{
    binop, exists, field, query, query_request, relation_field, relationship, target, value,
};

use crate::{connector::Connector, graphql_query, run_connector_query};

#[tokio::test]
async fn evaluates_field_name_that_requires_escaping_in_nested_expression() -> anyhow::Result<()> {
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
                query Filtering {
                  extendedJsonTestData(where: { value: { _regex: "hello" } }) {
                    type
                    value
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
async fn evaluates_exists_with_predicate() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::Chinook,
            query_request()
                .collection("Artist")
                .query(
                    query()
                        .predicate(exists(
                            ExistsInCollection::Related {
                                relationship: "albums".into(),
                                arguments: Default::default(),
                            },
                            binop("_iregex", target!("Title"), value!("Wild"))
                        ))
                        .fields([
                            field!("_id"),
                            field!("Name"),
                            relation_field!("albums" => "albums", query().fields([
                                field!("Title")
                            ]))
                        ]),
                )
                .relationships([("albums", relationship("Album", [("ArtistId", "ArtistId")]))])
        )
        .await?
    );
    Ok(())
}
