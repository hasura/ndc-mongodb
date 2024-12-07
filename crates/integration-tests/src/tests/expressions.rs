use insta::assert_yaml_snapshot;
use ndc_models::{ExistsInCollection, Expression};
use ndc_test_helpers::{
    array, asc, binop, exists, field, object, query, query_request, relation_field, relationship,
    target, value,
};

use crate::{connector::Connector, graphql_query, run_connector_query};

#[tokio::test]
async fn evaluates_field_name_that_requires_escaping() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query {
                  weirdFieldNames(where: { invalidName: { _eq: 3 } }) {
                    invalidName
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
async fn evaluates_field_name_that_requires_escaping_in_complex_expression() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
                query {
                  weirdFieldNames(
                    where: { 
                        _and: [
                            { invalidName: { _gt: 2 } },
                            { invalidName: { _lt: 4 } } 
                        ] 
                    }
                  ) {
                    invalidName
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
                            ]).order_by([asc!("Title")]))
                        ]),
                )
                .relationships([("albums", relationship("Album", [("ArtistId", "ArtistId")]))])
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn exists_with_predicate_with_escaped_field_name() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request().collection("weird_field_names").query(
                query()
                    .predicate(exists(
                        ExistsInCollection::NestedCollection {
                            column_name: "$invalid.array".into(),
                            arguments: Default::default(),
                            field_path: Default::default(),
                        },
                        binop("_lt", target!("$invalid.element"), value!(3)),
                    ))
                    .fields([
                        field!("_id"),
                        field!("invalid_array" => "$invalid.array", array!(object!([
                            field!("invalid_element" => "$invalid.element")
                        ])))
                    ])
                    .order_by([asc!("$invalid.name")]),
            )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn exists_in_nested_collection_without_predicate() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request().collection("nested_collection").query(
                query()
                    .predicate(Expression::Exists {
                        in_collection: ExistsInCollection::NestedCollection {
                            column_name: "staff".into(),
                            arguments: Default::default(),
                            field_path: Default::default(),
                        },
                        predicate: None,
                    })
                    .fields([field!("_id"), field!("institution")])
                    .order_by([asc!("institution")]),
            )
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn exists_in_nested_collection_without_predicate_with_escaped_field_name(
) -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request().collection("weird_field_names").query(
                query()
                    .predicate(Expression::Exists {
                        in_collection: ExistsInCollection::NestedCollection {
                            column_name: "$invalid.array".into(),
                            arguments: Default::default(),
                            field_path: Default::default(),
                        },
                        predicate: None,
                    })
                    .fields([
                        field!("_id"),
                        field!("invalid_array" => "$invalid.array", array!(object!([
                            field!("invalid_element" => "$invalid.element")
                        ])))
                    ])
                    .order_by([asc!("$invalid.name")]),
            )
        )
        .await?
    );
    Ok(())
}
