use configuration::MongoScalarType;
use mongodb::bson::Bson;
use mongodb_cli_plugin::type_from_bson;
use mongodb_support::{BsonScalarType, ExtendedJsonMode};
use ndc_query_plan::{self as plan, inline_object_types};
use plan::QueryContext;
use proptest::prelude::*;
use test_helpers::arb_bson::{arb_bson, arb_datetime};

use crate::mongo_query_plan::MongoConfiguration;

use super::{bson_to_json, json_to_bson};

proptest! {
    #[test]
    fn converts_bson_to_json_and_back(bson in arb_bson()) {
        let (schema_object_types, inferred_schema_type) = type_from_bson("test_object", &bson, false);
        let object_types = schema_object_types.into_iter().map(|(name, t)| (name, t.into())).collect();
        let inferred_type = inline_object_types(&object_types, &inferred_schema_type.into(), MongoConfiguration::lookup_scalar_type)?;
        let error_context = |msg: &str, source: String| TestCaseError::fail(format!("{msg}: {source}\ninferred type: {inferred_type:?}\nobject types: {object_types:?}"));

        // Test using Canonical mode because Relaxed mode loses some information, and so does not
        // round-trip precisely.
        let json = bson_to_json(ExtendedJsonMode::Canonical, &inferred_type, bson.clone()).map_err(|e| error_context("error converting bson to json", e.to_string()))?;
        let actual = json_to_bson(&inferred_type, json.clone()).map_err(|e| error_context("error converting json to bson", e.to_string()))?;
        prop_assert!(custom_eq(&actual, &bson),
            "`(left == right)`\nleft: `{:?}`\nright: `{:?}`\ninferred type: {:?}\nobject types: {:?}\njson_representation: {}",
            actual,
            bson,
            inferred_type,
            object_types,
            serde_json::to_string_pretty(&json).unwrap()
        )
    }
}

proptest! {
    #[test]
    fn converts_datetime_from_bson_to_json_and_back(d in arb_datetime()) {
        let t = plan::Type::Scalar(MongoScalarType::Bson(BsonScalarType::Date));
        let bson = Bson::DateTime(d);
        let json = bson_to_json(ExtendedJsonMode::Canonical, &t, bson.clone())?;
        let actual = json_to_bson(&t, json.clone())?;
        prop_assert_eq!(actual, bson, "json representation: {}", json)
    }
}

/// We are treating doubles as a superset of ints, so we need an equality check that allows
/// comparing those types.
fn custom_eq(a: &Bson, b: &Bson) -> bool {
    match (a, b) {
        (Bson::Double(a), Bson::Int32(b)) | (Bson::Int32(b), Bson::Double(a)) => *a == *b as f64,
        (Bson::Array(xs), Bson::Array(ys)) => {
            xs.len() == ys.len() && xs.iter().zip(ys.iter()).all(|(x, y)| custom_eq(x, y))
        }
        (Bson::Document(a), Bson::Document(b)) => {
            a.len() == b.len()
                && a.iter().all(|(key_a, value_a)| match b.get(key_a) {
                    Some(value_b) => custom_eq(value_a, value_b),
                    None => false,
                })
        }
        _ => a == b,
    }
}
