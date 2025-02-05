use crate::{connector::Connector, graphql_query, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{
    asc, binop, column, column_aggregate, dimension_column, exists, field, grouping,
    ordered_dimensions, query, query_request, related, relation_field, relationship, target, value,
};

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
async fn filters_by_non_null_field_of_related_collection() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              comments(
                limit: 10
                where: {movie: {title: {_is_null: false}}}
                order_by: {id: Asc}
              ) {
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
async fn filters_by_field_of_relationship_of_relationship() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              artist(where: {albums: {tracks: {name: {_eq: "Princess of the Dawn"}}}}) {
                name
                albums(order_by: {title: Asc}) {
                  title
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

#[tokio::test]
async fn looks_up_the_same_relation_twice_with_different_fields() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              artist(limit: 2, order_by: {id: Asc}) {
                albums1: albums(order_by: {title: Asc}) {
                  title
                }
                albums2: albums(order_by: {title: Asc}) {
                  tracks(order_by: {name: Asc}) {
                    name
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
async fn queries_through_relationship_with_null_value() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        graphql_query(
            r#"
            query {
              comments(where: {id: {_eq: "5a9427648b0beebeb69579cc"}}) { # this comment does not have a matching movie
                movie {
                  comments {
                    email
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
async fn joins_on_field_names_that_require_escaping() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request()
                .collection("weird_field_names")
                .query(
                    query()
                        .fields([
                            field!("invalid_name" => "$invalid.name"),
                            relation_field!("join" => "join", query().fields([
                              field!("invalid_name" => "$invalid.name")
                            ]))
                        ])
                        .order_by([asc!("_id")])
                )
                .relationships([(
                    "join",
                    relationship("weird_field_names", [("$invalid.name", &["$invalid.name"])])
                )])
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn joins_relationships_on_nested_key() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request()
                .collection("departments")
                .query(
                    query()
                        .predicate(exists(
                            related!("schools_departments"),
                            binop("_eq", target!("name"), value!("West Valley"))
                        ))
                        .fields([
                            relation_field!("departments" => "schools_departments", query().fields([
                              field!("name")
                            ]))
                        ])
                        .order_by([asc!("_id")])
                )
                .relationships([(
                    "schools_departments",
                    relationship("schools", [("_id", &["departments", "math_department_id"])])
                )])
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn gets_groups_through_relationship() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::Chinook,
            query_request()
                .collection("Album")
                .query(
                    query()
                    .limit(5)
                    .fields([relation_field!("tracks" => "album_tracks", query()
                      .groups(grouping()
                        .dimensions([dimension_column(column("Name").from_relationship("track_genre"))])
                          .aggregates([(
                            "average_price", column_aggregate("UnitPrice", "avg")
                          )])
                          .order_by(ordered_dimensions()),
                      )
                    )])
                )
                .relationships([
                    (
                        "album_tracks",
                        relationship("Track", [("albumId", &["albumId"])])
                    ),
                    (
                        "track_genre",
                        relationship("Genre", [("genreId", &["genreId"])]).object_type()
                    )
                ])
        )
        .await?
    );
    Ok(())
}
