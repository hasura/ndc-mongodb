use configuration::schema::Type;
use mongodb::bson::Bson;
use mongodb_cli_plugin::type_from_bson;
use mongodb_support::BsonScalarType;
use proptest::prelude::*;
use test_helpers::arb_bson::{arb_bson, arb_datetime};

use super::{bson_to_json, json_to_bson};

proptest! {
    #[test]
    fn converts_bson_to_json_and_back(bson in arb_bson()) {
        let (object_types, inferred_type) = type_from_bson("test_object", &bson);
        let json = bson_to_json(&inferred_type, &object_types, bson.clone())?;
        let actual = json_to_bson(&inferred_type, &object_types, json)?;
        prop_assert_eq!(actual, bson)
    }
}

proptest! {
    #[test]
    fn converts_datetime_from_bson_to_json_and_back(d in arb_datetime()) {
        let t = Type::Scalar(BsonScalarType::Date);
        let bson = Bson::DateTime(d);
        let json = bson_to_json(&t, &Default::default(), bson.clone())?;
        let actual = json_to_bson(&t, &Default::default(), json.clone())?;
        prop_assert_eq!(actual, bson, "json representation: {}", json)
    }
}
