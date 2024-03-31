use enum_iterator::Sequence as _;
use mongodb_support::BsonScalarType;
use proptest::prelude::*;

pub fn arb_bson_scalar_type() -> impl Strategy<Value = BsonScalarType> {
    (0..BsonScalarType::CARDINALITY)
        .prop_map(|n| enum_iterator::all::<BsonScalarType>().nth(n).unwrap())
}
