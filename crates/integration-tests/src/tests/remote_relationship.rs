use crate::{connector::Connector, graphql_query, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{
    and, asc, binop, column_aggregate, column_count_aggregate, dimension_column, field, grouping,
    ordered_dimensions, query, query_request, star_count_aggregate, target, value, variable,
};
use serde_json::json;

#[tokio::test]
async fn provides_source_and_target_for_remote_relationship() -> anyhow::Result<()> {
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

#[tokio::test]
async fn aggregates_request_with_variable_sets() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .collection("movies")
                .variables([[("year", json!(2014))]])
                .query(
                    query()
                        .predicate(binop("_eq", target!("year"), variable!(year)))
                        .aggregates([
                            (
                                "average_viewer_rating",
                                column_aggregate("tomatoes.viewer.rating", "avg").into(),
                            ),
                            column_count_aggregate!("rated_count" => "rated", distinct: true),
                            star_count_aggregate!("count"),
                        ])
                ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn aggregates_request_with_variable_sets_over_empty_collection_subset() -> anyhow::Result<()>
{
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .collection("movies")
                .variables([[("year", json!(2014))]])
                .query(
                    query()
                        .predicate(and([
                            binop("_eq", target!("year"), variable!(year)),
                            binop("_eq", target!("title"), value!("non-existent title")),
                        ]))
                        .aggregates([
                            (
                                "average_viewer_rating",
                                column_aggregate("tomatoes.viewer.rating", "avg").into(),
                            ),
                            column_count_aggregate!("rated_count" => "rated", distinct: true),
                            star_count_aggregate!("count"),
                        ])
                ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn provides_groups_for_variable_set() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .collection("movies")
                .variables([[("year", json!(2014))]])
                .query(
                    query()
                        .predicate(binop("_eq", target!("year"), variable!(year)))
                        .groups(
                            grouping()
                                .dimensions([dimension_column("rated")])
                                .aggregates([(
                                    "average_viewer_rating",
                                    column_aggregate("tomatoes.viewer.rating", "avg"),
                                ),])
                                .order_by(ordered_dimensions()),
                        ),
                ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn provides_fields_combined_with_groups_for_variable_set() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request()
                .collection("movies")
                .variables([[("year", json!(2014))]])
                .query(
                    query()
                        .predicate(binop("_eq", target!("year"), variable!(year)))
                        .fields([field!("title"), field!("rated")])
                        .order_by([asc!("_id")])
                        .groups(
                            grouping()
                                .dimensions([dimension_column("rated")])
                                .aggregates([(
                                    "average_viewer_rating",
                                    column_aggregate("tomatoes.viewer.rating", "avg"),
                                ),])
                                .order_by(ordered_dimensions()),
                        )
                        .limit(3),
                ),
        )
        .await?
    );
    Ok(())
}
