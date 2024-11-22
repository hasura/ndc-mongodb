use configuration::schema::Type;
use enum_iterator::Sequence as _;
use mongodb_support::BsonScalarType;
use proptest::{prelude::*, string::string_regex};

pub fn arb_bson_scalar_type() -> impl Strategy<Value = BsonScalarType> {
    (0..BsonScalarType::CARDINALITY)
        .prop_map(|n| enum_iterator::all::<BsonScalarType>().nth(n).unwrap())
}

pub fn arb_type() -> impl Strategy<Value = Type> {
    let leaf = prop_oneof![
        arb_bson_scalar_type().prop_map(Type::Scalar),
        arb_object_type_name().prop_map(Type::Object),
        arb_object_type_name().prop_map(|name| Type::Predicate {
            object_type_name: name.into()
        })
    ];
    leaf.prop_recursive(3, 10, 10, |inner| {
        prop_oneof![
            inner.clone().prop_map(|t| Type::ArrayOf(Box::new(t))),
            inner.prop_map(|t| Type::Nullable(Box::new(t)))
        ]
    })
}

fn arb_object_type_name() -> impl Strategy<Value = String> {
    string_regex(r#"[a-zA-Z_][a-zA-Z0-9_]*"#)
        .unwrap()
        .prop_filter(
            "object type names must not collide with scalar type names",
            |name| !enum_iterator::all::<BsonScalarType>().any(|t| t.bson_name() == name),
        )
}
