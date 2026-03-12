use crate::{connector::Connector, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{asc, field, query, query_request};

#[tokio::test]
async fn selects_fields_with_weird_aliases() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::SampleMflix,
            query_request().collection("movies").query(
                query()
                    .fields([field!("foo.bar" => "title"), field!("year")])
                    .limit(10)
                    .order_by([asc!("_id")]),
            )
        )
        .await?
    );
    Ok(())
}
