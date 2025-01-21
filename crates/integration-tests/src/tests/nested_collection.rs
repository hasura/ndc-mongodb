use crate::{connector::Connector, run_connector_query};
use insta::assert_yaml_snapshot;
use ndc_test_helpers::{
    array, asc, binop, exists, exists_in_nested, field, object, query, query_request, target, value,
};

#[tokio::test]
async fn exists_in_nested_collection() -> anyhow::Result<()> {
    assert_yaml_snapshot!(
        run_connector_query(
            Connector::TestCases,
            query_request().collection("nested_collection").query(
                query()
                    .predicate(exists(
                        exists_in_nested("staff"),
                        binop("_eq", target!("name"), value!("Alyx"))
                    ))
                    .fields([
                        field!("institution"),
                        field!("staff" => "staff", array!(object!([field!("name")]))),
                    ])
                    .order_by([asc!("_id")])
            )
        )
        .await?
    );
    Ok(())
}
