use configuration::MongoScalarType;
use ndc_query_plan::{ObjectField, ObjectType, Type};
use proptest::{collection::btree_map, prelude::*};

use crate::arb_type::arb_bson_scalar_type;

pub fn arb_plan_type() -> impl Strategy<Value = Type<MongoScalarType>> {
    let leaf = arb_plan_scalar_type().prop_map(Type::Scalar);
    leaf.prop_recursive(3, 10, 10, |inner| {
        prop_oneof![
            inner.clone().prop_map(|t| Type::ArrayOf(Box::new(t))),
            inner.clone().prop_map(|t| Type::Nullable(Box::new(t))),
            (
                any::<Option<String>>(),
                btree_map(any::<String>().prop_map_into(), inner, 1..=10)
            )
                .prop_map(|(name, field_types)| Type::Object(ObjectType {
                    name: name.map(|n| n.into()),
                    fields: field_types
                        .into_iter()
                        .map(|(name, t)| (
                            name,
                            ObjectField {
                                r#type: t,
                                parameters: Default::default()
                            }
                        ))
                        .collect(),
                }))
        ]
    })
}

fn arb_plan_scalar_type() -> impl Strategy<Value = MongoScalarType> {
    prop_oneof![
        arb_bson_scalar_type().prop_map(MongoScalarType::Bson),
        Just(MongoScalarType::ExtendedJSON)
    ]
}
