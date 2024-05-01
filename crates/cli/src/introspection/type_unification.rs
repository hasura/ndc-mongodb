/// This module contains functions for unifying types.
/// This is useful when deriving a schema from set of sample documents.
/// It allows the information in the schemas derived from several documents to be combined into one schema.
///
use configuration::{
    schema::{self, Type},
    WithName,
};
use indexmap::IndexMap;
use itertools::Itertools as _;
use mongodb_support::{align::align, BsonScalarType::*};
use std::string::String;

type ObjectField = WithName<schema::ObjectField>;
type ObjectType = WithName<schema::ObjectType>;

/// Unify two types.
/// This is computing the join (or least upper bound) of the two types in a lattice
/// where `ExtendedJSON` is the Top element, Scalar(Undefined) is the Bottom element, and Nullable(T) >= T for all T.
pub fn unify_type(type_a: Type, type_b: Type) -> Type {
    let result_type = match (type_a, type_b) {
        // Union of any type with ExtendedJSON is ExtendedJSON
        (Type::ExtendedJSON, _) => Type::ExtendedJSON,
        (_, Type::ExtendedJSON) => Type::ExtendedJSON,

        // If one type is undefined, the union is the other type.
        // This is used as the base case when inferring array types from documents.
        (Type::Scalar(Undefined), type_b) => type_b,
        (type_a, Type::Scalar(Undefined)) => type_a,

        // A Nullable type will unify with another type iff the underlying type is unifiable.
        // The resulting type will be Nullable.
        (Type::Nullable(nullable_type_a), type_b) => {
            let result_type = unify_type(*nullable_type_a, type_b);
            result_type.make_nullable()
        }
        (type_a, Type::Nullable(nullable_type_b)) => {
            let result_type = unify_type(type_a, *nullable_type_b);
            result_type.make_nullable()
        }

        // Union of any type with Null is the Nullable version of that type
        (Type::Scalar(Null), type_b) => type_b.make_nullable(),
        (type_a, Type::Scalar(Null)) => type_a.make_nullable(),

        // Scalar types unify if they are the same type.
        // If they are diffferent then the union is ExtendedJSON.
        (Type::Scalar(scalar_a), Type::Scalar(scalar_b)) => {
            if scalar_a == scalar_b {
                Type::Scalar(scalar_a)
            } else {
                Type::ExtendedJSON
            }
        }

        // Object types unify if they have the same name.
        // If they are diffferent then the union is ExtendedJSON.
        (Type::Object(object_a), Type::Object(object_b)) => {
            if object_a == object_b {
                Type::Object(object_a)
            } else {
                Type::ExtendedJSON
            }
        }

        // Array types unify iff their element types unify.
        (Type::ArrayOf(elem_type_a), Type::ArrayOf(elem_type_b)) => {
            let elem_type = unify_type(*elem_type_a, *elem_type_b);
            Type::ArrayOf(Box::new(elem_type))
        }

        // Anything else gives ExtendedJSON
        (_, _) => Type::ExtendedJSON,
    };

    result_type
}

pub fn make_nullable_field(field: ObjectField) -> ObjectField {
    WithName::named(
        field.name,
        schema::ObjectField {
            r#type: field.value.r#type.make_nullable(),
            description: field.value.description,
        },
    )
}

/// Unify two `ObjectType`s.
/// Any field that appears in only one of the `ObjectType`s will be made nullable.
fn unify_object_type(object_type_a: ObjectType, object_type_b: ObjectType) -> ObjectType {
    let field_map_a: IndexMap<String, ObjectField> = object_type_a
        .value
        .fields
        .into_iter()
        .map_into::<ObjectField>()
        .map(|o| (o.name.to_owned(), o))
        .collect();
    let field_map_b: IndexMap<String, ObjectField> = object_type_b
        .value
        .fields
        .into_iter()
        .map_into::<ObjectField>()
        .map(|o| (o.name.to_owned(), o))
        .collect();

    let merged_field_map = align(
        field_map_a,
        field_map_b,
        make_nullable_field,
        make_nullable_field,
        unify_object_field,
    );

    WithName::named(
        object_type_a.name,
        schema::ObjectType {
            fields: merged_field_map
                .into_values()
                .map(WithName::into_name_value_pair)
                .collect(),
            description: object_type_a
                .value
                .description
                .or(object_type_b.value.description),
        },
    )
}

/// Unify the types of two `ObjectField`s.
/// If the types are not unifiable then return an error.
fn unify_object_field(object_field_a: ObjectField, object_field_b: ObjectField) -> ObjectField {
    WithName::named(
        object_field_a.name,
        schema::ObjectField {
            r#type: unify_type(object_field_a.value.r#type, object_field_b.value.r#type),
            description: object_field_a
                .value
                .description
                .or(object_field_b.value.description),
        },
    )
}

/// Unify two sets of `ObjectType`s.
/// Any `ObjectType` that appears in only one set will be unchanged in the output.
/// Any type that appears in both sets will be unified using `unify_object_type`.
pub fn unify_object_types(
    object_types_a: Vec<ObjectType>,
    object_types_b: Vec<ObjectType>,
) -> Vec<ObjectType> {
    let type_map_a: IndexMap<String, ObjectType> = object_types_a
        .into_iter()
        .map(|t| (t.name.to_owned(), t))
        .collect();
    let type_map_b: IndexMap<String, ObjectType> = object_types_b
        .into_iter()
        .map(|t| (t.name.to_owned(), t))
        .collect();

    let merged_type_map = align(
        type_map_a,
        type_map_b,
        std::convert::identity,
        std::convert::identity,
        unify_object_type,
    );

    merged_type_map.into_values().collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use super::{unify_object_type, unify_type};
    use configuration::{
        schema::{self, Type},
        WithName,
    };
    use mongodb_support::BsonScalarType;
    use proptest::{collection::hash_map, prelude::*};
    use test_helpers::arb_type;

    #[test]
    fn test_unify_scalar() -> Result<(), anyhow::Error> {
        let expected = Type::Scalar(BsonScalarType::Int);
        let actual = unify_type(
            Type::Scalar(BsonScalarType::Int),
            Type::Scalar(BsonScalarType::Int),
        );
        assert_eq!(expected, actual);
        Ok(())
    }

    #[test]
    fn test_unify_scalar_error() -> Result<(), anyhow::Error> {
        let expected = Type::ExtendedJSON;
        let actual = unify_type(
            Type::Scalar(BsonScalarType::Int),
            Type::Scalar(BsonScalarType::String),
        );
        assert_eq!(expected, actual);
        Ok(())
    }

    fn is_nullable(t: &Type) -> bool {
        matches!(
            t,
            Type::Scalar(BsonScalarType::Null) | Type::Nullable(_) | Type::ExtendedJSON
        )
    }

    proptest! {
        #[test]
        fn test_type_unifies_with_itself_and_normalizes(t in arb_type()) {
            let u = unify_type(t.clone(), t.clone());
            prop_assert_eq!(t.normalize_type(), u)
        }
    }

    proptest! {
        #[test]
        fn test_unify_type_is_commutative(ta in arb_type(), tb in arb_type()) {
            let result_a_b = unify_type(ta.clone(), tb.clone());
            let result_b_a = unify_type(tb, ta);
            prop_assert_eq!(result_a_b, result_b_a)
        }
    }

    proptest! {
        #[test]
        fn test_unify_type_is_associative(ta in arb_type(), tb in arb_type(), tc in arb_type()) {
            let result_lr = unify_type(unify_type(ta.clone(), tb.clone()), tc.clone());
            let result_rl = unify_type(ta, unify_type(tb, tc));
            prop_assert_eq!(result_lr, result_rl)
        }
    }

    proptest! {
        #[test]
        fn test_undefined_is_left_identity(t in arb_type()) {
            let u = unify_type(Type::Scalar(BsonScalarType::Undefined), t.clone());
            prop_assert_eq!(t.normalize_type(), u)
        }
    }

    proptest! {
        #[test]
        fn test_undefined_is_right_identity(t in arb_type()) {
            let u = unify_type(t.clone(), Type::Scalar(BsonScalarType::Undefined));
            prop_assert_eq!(t.normalize_type(), u)
        }
    }

    proptest! {
        #[test]
        fn test_any_left(t in arb_type()) {
            let u = unify_type(Type::ExtendedJSON, t);
            prop_assert_eq!(Type::ExtendedJSON, u)
        }
    }

    proptest! {
        #[test]
        fn test_any_right(t in arb_type()) {
            let u = unify_type(t, Type::ExtendedJSON);
            prop_assert_eq!(Type::ExtendedJSON, u)
        }
    }

    fn type_hash_map() -> impl Strategy<Value = HashMap<String, Type>> {
        hash_map(".*", arb_type(), 0..10)
    }

    proptest! {
        #[test]
        fn test_object_type_unification(left in type_hash_map(), right in type_hash_map(), shared in type_hash_map()) {
            let mut left_fields = left.clone();
            let mut right_fields: HashMap<String, Type> = right.clone().into_iter().filter(|(k, _)| !left_fields.contains_key(k)).collect();
            for (k, v) in shared.clone() {
                left_fields.insert(k.clone(), v.clone());
                right_fields.insert(k, v);
            }

            let name = "foo";
            let left_object = WithName::named(name.to_owned(), schema::ObjectType {
                fields: left_fields.into_iter().map(|(k, v)| (k, schema::ObjectField{r#type: v, description: None})).collect(),
                description: None
            });
            let right_object = WithName::named(name.to_owned(), schema::ObjectType {
                fields: right_fields.into_iter().map(|(k, v)| (k, schema::ObjectField{r#type: v, description: None})).collect(),
                description: None
            });
            let result = unify_object_type(left_object, right_object);

            for field in result.value.named_fields() {
                // Any fields not shared between the two input types should be nullable.
                if !shared.contains_key(field.name) {
                    assert!(is_nullable(&field.value.r#type), "Found a non-shared field that is not nullable")
                }
            }

            // All input fields must appear in the result.
            let fields: HashSet<String> = result.value.fields.into_keys().collect();
            assert!(left.into_keys().chain(right.into_keys()).chain(shared.into_keys()).all(|k| fields.contains(&k)),
                "Missing field in result type")
        }
    }
}
