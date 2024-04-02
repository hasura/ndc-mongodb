use configuration::schema::Type;
use enum_iterator::Sequence as _;
use mongodb_support::BsonScalarType;
use proptest::prelude::*;

pub fn arb_bson_scalar_type() -> impl Strategy<Value = BsonScalarType> {
    (0..BsonScalarType::CARDINALITY)
        .prop_map(|n| enum_iterator::all::<BsonScalarType>().nth(n).unwrap())
}

pub fn arb_type() -> impl Strategy<Value = Type> {
    let leaf = prop_oneof![
        arb_bson_scalar_type().prop_map(Type::Scalar),
        any::<String>().prop_map(Type::Object)
    ];
    leaf.prop_recursive(3, 10, 10, |inner| {
        prop_oneof![
            inner.clone().prop_map(|t| Type::ArrayOf(Box::new(t))),
            inner.prop_map(|t| Type::Nullable(Box::new(t)))
        ]
    })
}
