use std::str::FromStr;

use anyhow::anyhow;
use configuration::schema::Type;
use introspection::type_from_bson;
use mongodb::bson::{self, Bson};
use mongodb_support::BsonScalarType;
use pretty_assertions::assert_eq;
use proptest::prelude::*;
use test_helpers::{arb_bson_with_options, ArbBsonOptions};

use super::{bson_to_json, json_to_bson};

// bson_to_json should round-trip with json_to_bson - but note that round-trips do not work for
// values where the inferred type includes `Any` because in those cases we lose the necessary type
// information to convert back to BSON losslessly. `Any` appears in an inferred type when a value
// includes an array with elements of different types. So we limit tests to arrays with uniform
// types.
proptest! {
    #[test]
    fn converts_bson_to_json_and_back(bson in arb_bson_with_options(ArbBsonOptions { heterogeneous_arrays: false, ..Default::default() })) {
        let (object_types, inferred_type) = type_from_bson("test_object", &bson);
        let json = bson_to_json(bson.clone())?;
        let actual = json_to_bson(&inferred_type, &object_types, json)?;
        prop_assert_eq!(actual, bson)
    }
}

proptest! {
    #[test]
    fn converts_decimals_round_trip(bytes in any::<[u8; 128 / 8]>()) {
        let expected = bson::Decimal128::from_bytes(bytes);
        let bson = Bson::Decimal128(expected);
        let json = bson_to_json(bson.clone())?;
        let result = json_to_bson(&Type::Scalar(BsonScalarType::Decimal), &Default::default(), json)?;
        let actual = match result {
            Bson::Decimal128(d) => d,
            _ => return Err(TestCaseError::fail("wrong type")),
        };
        prop_assert_eq!(actual, expected)
    }
}

#[test]
fn converts_decimal_round_trip() -> anyhow::Result<()> {
    let bytes = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 96];
    let raw_decimal = bson::Decimal128::from_bytes(bytes);

    let expected = FromStr::from_str(&raw_decimal.to_string()).unwrap();

    let bson = Bson::Decimal128(expected);
    let json = bson_to_json(bson.clone())?;
    let result = json_to_bson(
        &Type::Scalar(BsonScalarType::Decimal),
        &Default::default(),
        json,
    )?;
    let actual = match result {
        Bson::Decimal128(d) => d,
        _ => return Err(anyhow!("wrong type")),
    };
    println!("actual: {} ; expected = {}", actual.to_string(), serde_json::to_value(expected).unwrap());
    assert_eq!(actual.bytes(), expected.bytes());
    assert_eq!(actual, expected);
    Ok(())
}
