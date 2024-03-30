use introspection::test_helpers::{arb_bson, type_from_bson};
use proptest::prelude::*;

use super::{bson_to_json, json_to_bson};

proptest! {
    #[test]
    fn converts_bson_to_json_and_back(bson in arb_bson()) {
        let (object_types, inferred_type) = type_from_bson("test_object", bson);
        let json = bson_to_json(&bson).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        let actual = json_to_bson(&inferred_type, &object_types, json).map_err(|e| TestCaseError::Fail(e.to_string().into()))?;
        prop_assert_eq!(actual, bson)
    }
}
