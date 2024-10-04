use insta::assert_yaml_snapshot;
use ndc_models::ExistsInCollection;
use ndc_test_helpers::{
    binop, exists, field, query, query_request, relation_field, relationship, target, value,
};

use crate::{connector::Connector, graphql_query, run_connector_query};

#[tokio::test]
async fn evaluates_field_name_that_requires_escaping_in_nested_expression() -> anyhow::Result<()> {
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
