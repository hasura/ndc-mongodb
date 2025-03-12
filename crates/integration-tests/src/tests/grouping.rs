use insta::assert_yaml_snapshot;
use ndc_test_helpers::{
    and, asc, binop, column_aggregate, column_count_aggregate, dimension_column, field, grouping, or, ordered_dimensions, query, query_request, star_count_aggregate, target, value
};

use crate::{connector::Connector, run_connector_query};

#[tokio::test]
async fn runs_single_column_aggregate_on_groups() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    // The predicate avoids an error when encountering documents where `year` is
                    // a string instead of a number.
                    .predicate(or([
                        binop("_gt", target!("year"), value!(0)),
                        binop("_lte", target!("year"), value!(0)),
                    ]))
                    .order_by([asc!("_id")])
                    .limit(10)
                    .groups(
                        grouping()
                            .dimensions([dimension_column("year")])
                            .aggregates([
                                (
                                    "average_viewer_rating",
                                    column_aggregate("tomatoes.viewer.rating", "avg"),
                                ),
                                ("max_runtime", column_aggregate("runtime", "max")),
                            ])
                            .order_by(ordered_dimensions()),
                    ),
            ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn counts_column_values_in_groups() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    .predicate(and([
                        binop("_gt", target!("year"), value!(1920)),
                        binop("_lte", target!("year"), value!(1923)),
                    ]))
                    .groups(
                        grouping()
                            .dimensions([dimension_column("rated")])
                            .aggregates([
                                // The distinct count should be 3 or less because we filtered to only 3 years
                                column_count_aggregate!("year_distinct_count" => "year", distinct: true),
                                column_count_aggregate!("year_count" => "year", distinct: false),
                                star_count_aggregate!("count"),
                            ])
                            .order_by(ordered_dimensions()),
                    ),
            ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn groups_by_multiple_dimensions() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    .predicate(binop("_lt", target!("year"), value!(1950)))
                    .order_by([asc!("_id")])
                    .limit(10)
                    .groups(
                        grouping()
                            .dimensions([
                                dimension_column("year"),
                                dimension_column("languages"),
                                dimension_column("rated"),
                            ])
                            .aggregates([(
                                "average_viewer_rating",
                                column_aggregate("tomatoes.viewer.rating", "avg"),
                            )])
                            .order_by(ordered_dimensions()),
                    ),
            ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn combines_aggregates_and_groups_in_one_query() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    .predicate(binop("_gte", target!("year"), value!(2000)))
                    .order_by([asc!("_id")])
                    .limit(10)
                    .aggregates([(
                        "average_viewer_rating",
                        column_aggregate("tomatoes.viewer.rating", "avg")
                    )])
                    .groups(
                        grouping()
                            .dimensions([dimension_column("year"),])
                            .aggregates([(
                                "average_viewer_rating_by_year",
                                column_aggregate("tomatoes.viewer.rating", "avg"),
                            )])
                            .order_by(ordered_dimensions()),
                    ),
            ),
        )
        .await?
    );
    Ok(())
}

#[tokio::test]
async fn combines_fields_and_groups_in_one_query() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    // The predicate avoids an error when encountering documents where `year` is
                    // a string instead of a number.
                    .predicate(or([
                        binop("_gt", target!("year"), value!(0)),
                        binop("_lte", target!("year"), value!(0)),
                    ]))
                    .order_by([asc!("_id")])
                    .limit(3)
                    .fields([field!("title"), field!("year")])
                    .order_by([asc!("_id")])
                    .groups(
                        grouping()
                            .dimensions([dimension_column("year")])
                            .aggregates([(
                                "average_viewer_rating_by_year",
                                column_aggregate("tomatoes.viewer.rating", "avg"),
                            )])
                            .order_by(ordered_dimensions()),
                    )
            ),
        )
        .await?
    );
    Ok(())
}
