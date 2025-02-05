use insta::assert_yaml_snapshot;
use ndc_test_helpers::{
    binop, column_aggregate, dimension_column, field, grouping, is_in, query, query_request,
    target, value,
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
                    .predicate(binop("_gte", target!("year"), value!(2000)))
                    .groups(
                        grouping()
                            .dimensions([dimension_column("year")])
                            .aggregates([
                                (
                                    "average_viewer_rating",
                                    column_aggregate("tomatoes.viewer.rating", "avg"),
                                ),
                                ("max_runtime", column_aggregate("runtime", "max")),
                            ]),
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
                    .predicate(binop("_lt", target!("year"), value!(1920)))
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
                            )]),
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
                            )]),
                    ),
            ),
        )
        .await?
    );
    Ok(())
}
