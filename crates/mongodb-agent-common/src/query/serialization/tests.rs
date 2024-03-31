use std::str::FromStr;

use introspection::type_from_bson;
use mongodb::bson::Bson;
use pretty_assertions::assert_eq;
use proptest::prelude::*;
use test_helpers::arb_bson;

use super::{bson_to_json, json_to_bson};

proptest! {
    #[test]
    fn converts_bson_to_json_and_back(bson in arb_bson()) {
        let (object_types, inferred_type) = type_from_bson("test_object", &bson);
        let json = bson_to_json(bson.clone())?;
        let actual = json_to_bson(&inferred_type, &object_types, json)?;
        prop_assert_eq!(actual, bson)
    }
}

#[test]
fn converts_heterogeneous_array_from_bson_to_json_and_back() -> anyhow::Result<()> {
    let bson = Bson::Array(vec![
        Bson::Int32(1),
        Bson::Int64(2),
        Bson::Double(3.0),
        Bson::Decimal128(FromStr::from_str("4.0").unwrap()),
    ]);
    let (object_types, inferred_type) = type_from_bson("test_object", &bson);
    println!("inferred_type = {inferred_type:?}");
    let json = bson_to_json(bson.clone())?;
    println!("json = {}", serde_json::to_string_pretty(&json).unwrap());
    let actual = json_to_bson(&inferred_type, &object_types, json)?;
    assert_eq!(actual, bson);
    Ok(())
}
