use introspection::type_from_bson;
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
