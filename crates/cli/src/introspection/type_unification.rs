/// This module contains functions for unifying types.
/// This is useful when deriving a schema from set of sample documents.
/// It allows the information in the schemas derived from several documents to be combined into one schema.
///
use configuration::{
    schema::{ObjectField, ObjectType, Type},
    Schema,
};
use indexmap::IndexMap;
use mongodb_support::{
    align::align_with_result,
    BsonScalarType::{self, *},
};
use std::{
    fmt::{self, Display},
    string::String,
};
use thiserror::Error;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TypeUnificationContext {
    object_type_name: String,
    field_name: String,
}

impl TypeUnificationContext {
    pub fn new(object_type_name: &str, field_name: &str) -> Self {
        TypeUnificationContext {
            object_type_name: object_type_name.to_owned(),
            field_name: field_name.to_owned(),
        }
    }
}

impl Display for TypeUnificationContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "object type: {}, field: {}",
            self.object_type_name, self.field_name
        )
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TypeUnificationError {
    ScalarType(TypeUnificationContext, BsonScalarType, BsonScalarType),
    ObjectType(String, String),
    TypeKind(Type, Type),
}

impl Display for TypeUnificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScalarType(context, scalar_a, scalar_b) => write!(
                f,
                "Scalar type mismatch {} {} at {}",
                scalar_a.bson_name(),
                scalar_b.bson_name(),
                context
            ),
            Self::ObjectType(object_a, object_b) => {
                write!(f, "Object type mismatch {} {}", object_a, object_b)
            }
            Self::TypeKind(type_a, type_b) => {
                write!(f, "Type mismatch {:?} {:?}", type_a, type_b)
            }
        }
    }
}

pub type TypeUnificationResult<T> = Result<T, TypeUnificationError>;

/// Unify two types.
/// Return an error if the types are not unifiable.
pub fn unify_type(
    context: TypeUnificationContext,
    type_a: Type,
    type_b: Type,
) -> TypeUnificationResult<Type> {
    match (type_a, type_b) {
        // If one type is undefined, the union is the other type.
        // This is used as the base case when inferring array types from documents.
        (Type::Scalar(Undefined), type_b) => Ok(type_b),
        (type_a, Type::Scalar(Undefined)) => Ok(type_a),

        // Union of any type with Null is the Nullable version of that type
        (Type::Scalar(Null), type_b) => Ok(make_nullable(type_b)),
        (type_a, Type::Scalar(Null)) => Ok(make_nullable(type_a)),

        // Scalar types only unify if they are the same type.
        (Type::Scalar(scalar_a), Type::Scalar(scalar_b)) => {
            if scalar_a == scalar_b {
                Ok(Type::Scalar(scalar_a))
            } else {
                Err(TypeUnificationError::ScalarType(
                    context, scalar_a, scalar_b,
                ))
            }
        }

        // Object types only unify if they have the same name.
        (Type::Object(object_a), Type::Object(object_b)) => {
            if object_a == object_b {
                Ok(Type::Object(object_a))
            } else {
                Err(TypeUnificationError::ObjectType(object_a, object_b))
            }
        }

        // Array types unify iff their element types unify.
        (Type::ArrayOf(elem_type_a), Type::ArrayOf(elem_type_b)) => {
            let elem_type = unify_type(context, *elem_type_a, *elem_type_b)?;
            Ok(Type::ArrayOf(Box::new(elem_type)))
        }

        // A Nullable type will unify with another type iff the underlying type is unifiable.
        // The resulting type will be Nullable.
        (Type::Nullable(nullable_type_a), type_b) => {
            let result_type = unify_type(context, *nullable_type_a, type_b)?;
            Ok(make_nullable(result_type))
        }
        (type_a, Type::Nullable(nullable_type_b)) => {
            let result_type = unify_type(context, type_a, *nullable_type_b)?;
            Ok(make_nullable(result_type))
        }

        // Anything else is a unification error.
        (type_a, type_b) => Err(TypeUnificationError::TypeKind(type_a, type_b)),
    }
    .map(normalize_type)
}

fn normalize_type(t: Type) -> Type {
    match t {
        Type::Scalar(s) => Type::Scalar(s),
        Type::Object(o) => Type::Object(o),
        Type::ArrayOf(a) => Type::ArrayOf(Box::new(normalize_type(*a))),
        Type::Nullable(n) => match *n {
            Type::Scalar(BsonScalarType::Null) => Type::Scalar(BsonScalarType::Null),
            Type::Nullable(t) => normalize_type(Type::Nullable(t)),
            t => Type::Nullable(Box::new(normalize_type(t))),
        },
    }
}

fn make_nullable(t: Type) -> Type {
    match t {
        Type::Nullable(t) => Type::Nullable(t),
        Type::Scalar(BsonScalarType::Null) => Type::Scalar(BsonScalarType::Null),
        t => Type::Nullable(Box::new(t)),
    }
}

fn make_nullable_field<E>(field: ObjectField) -> Result<ObjectField, E> {
    Ok(ObjectField {
        name: field.name,
        r#type: make_nullable(field.r#type),
        description: field.description,
    })
}

/// Unify two `ObjectType`s.
/// Any field that appears in only one of the `ObjectType`s will be made nullable.
fn unify_object_type(
    object_type_a: ObjectType,
    object_type_b: ObjectType,
) -> TypeUnificationResult<ObjectType> {
    let field_map_a: IndexMap<String, ObjectField> = object_type_a
        .fields
        .into_iter()
        .map(|o| (o.name.to_owned(), o))
        .collect();
    let field_map_b: IndexMap<String, ObjectField> = object_type_b
        .fields
        .into_iter()
        .map(|o| (o.name.to_owned(), o))
        .collect();

    let merged_field_map = align_with_result(
        field_map_a,
        field_map_b,
        make_nullable_field,
        make_nullable_field,
        |field_a, field_b| unify_object_field(&object_type_a.name, field_a, field_b),
    )?;

    Ok(ObjectType {
        name: object_type_a.name,
        fields: merged_field_map.into_values().collect(),
        description: object_type_a.description.or(object_type_b.description),
    })
}

/// Unify the types of two `ObjectField`s.
/// If the types are not unifiable then return an error.
fn unify_object_field(
    object_type_name: &str,
    object_field_a: ObjectField,
    object_field_b: ObjectField,
) -> TypeUnificationResult<ObjectField> {
    let context = TypeUnificationContext::new(object_type_name, &object_field_a.name);
    Ok(ObjectField {
        name: object_field_a.name,
        r#type: unify_type(context, object_field_a.r#type, object_field_b.r#type)?,
        description: object_field_a.description.or(object_field_b.description),
    })
}

/// Unify two sets of `ObjectType`s.
/// Any `ObjectType` that appears in only one set will be unchanged in the output.
/// Any type that appears in both sets will be unified using `unify_object_type`.
pub fn unify_object_types(
    object_types_a: Vec<ObjectType>,
    object_types_b: Vec<ObjectType>,
) -> TypeUnificationResult<Vec<ObjectType>> {
    let type_map_a: IndexMap<String, ObjectType> = object_types_a
        .into_iter()
        .map(|t| (t.name.to_owned(), t))
        .collect();
    let type_map_b: IndexMap<String, ObjectType> = object_types_b
        .into_iter()
        .map(|t| (t.name.to_owned(), t))
        .collect();

    let merged_type_map = align_with_result(type_map_a, type_map_b, Ok, Ok, unify_object_type)?;

    Ok(merged_type_map.into_values().collect())
}

/// Unify two schemas. Assumes that the schemas describe mutually exclusive sets of collections.
pub fn unify_schema(schema_a: Schema, schema_b: Schema) -> Schema {
    let collections = schema_a
        .collections
        .into_iter()
        .chain(schema_b.collections)
        .collect();
    let object_types = schema_a
        .object_types
        .into_iter()
        .chain(schema_b.object_types)
        .collect();
    Schema {
        collections,
        object_types,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_type, unify_type, TypeUnificationContext, TypeUnificationError};
    use configuration::schema::Type;
    use mongodb_support::BsonScalarType;
    use proptest::prelude::*;

    #[test]
    fn test_unify_scalar() -> Result<(), anyhow::Error> {
        let context = TypeUnificationContext::new("foo", "bar");
        let expected = Ok(Type::Scalar(BsonScalarType::Int));
        let actual = unify_type(
            context,
            Type::Scalar(BsonScalarType::Int),
            Type::Scalar(BsonScalarType::Int),
        );
        assert_eq!(expected, actual);
        Ok(())
    }

    #[test]
    fn test_unify_scalar_error() -> Result<(), anyhow::Error> {
        let context = TypeUnificationContext::new("foo", "bar");
        let expected = Err(TypeUnificationError::ScalarType(
            context.clone(),
            BsonScalarType::Int,
            BsonScalarType::String,
        ));
        let actual = unify_type(
            context,
            Type::Scalar(BsonScalarType::Int),
            Type::Scalar(BsonScalarType::String),
        );
        assert_eq!(expected, actual);
        Ok(())
    }

    // #[test]
    // fn test_unify_object_type() -> Result<(), anyhow::Error> {
    //     let name = "foo";
    //     let description = "the type foo";
    //     let object_type_a = ObjectType {
    //         name: name.to_owned(),
    //         fields: vec![],
    //         description: Some(description.to_owned()),
    //     };

    //     Ok(())
    // }

    prop_compose! {
        fn arb_type_unification_context()(object_type_name in any::<String>(), field_name in any::<String>()) -> TypeUnificationContext {
            TypeUnificationContext { object_type_name, field_name }
        }
    }

    fn arb_bson_scalar_type() -> impl Strategy<Value = BsonScalarType> {
        prop_oneof![
            Just(BsonScalarType::Double),
            Just(BsonScalarType::Decimal),
            Just(BsonScalarType::Int),
            Just(BsonScalarType::Long),
            Just(BsonScalarType::String),
            Just(BsonScalarType::Date),
            Just(BsonScalarType::Timestamp),
            Just(BsonScalarType::BinData),
            Just(BsonScalarType::ObjectId),
            Just(BsonScalarType::Bool),
            Just(BsonScalarType::Null),
            Just(BsonScalarType::Regex),
            Just(BsonScalarType::Javascript),
            Just(BsonScalarType::JavascriptWithScope),
            Just(BsonScalarType::MinKey),
            Just(BsonScalarType::MaxKey),
            Just(BsonScalarType::Undefined),
            Just(BsonScalarType::DbPointer),
            Just(BsonScalarType::Symbol),
        ]
    }

    fn arb_type() -> impl Strategy<Value = Type> {
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

    fn swap_error(err: TypeUnificationError) -> TypeUnificationError {
        match err {
            TypeUnificationError::ScalarType(c, a, b) => TypeUnificationError::ScalarType(c, b, a),
            TypeUnificationError::ObjectType(a, b) => TypeUnificationError::ObjectType(b, a),
            TypeUnificationError::TypeKind(a, b) => TypeUnificationError::TypeKind(b, a),
        }
    }

    proptest! {
        #[test]
        fn test_type_unifies_with_itself_and_normalizes(c in arb_type_unification_context(), t in arb_type()) {
            let u = unify_type(c, t.clone(), t.clone());
            prop_assert_eq!(Ok(normalize_type(t)), u)
        }
    }

    proptest! {
        #[test]
        fn test_unify_type_is_commutative(c in arb_type_unification_context(), ta in arb_type(), tb in arb_type()) {
            let result_a_b = unify_type(c.clone(), ta.clone(), tb.clone());
            let result_b_a = unify_type(c, tb, ta);
            prop_assert_eq!(result_a_b, result_b_a.map_err(swap_error))
        }
    }

    proptest! {
        #[test]
        fn test_unify_type_is_associative(c in arb_type_unification_context(), ta in arb_type(), tb in arb_type(), tc in arb_type()) {
            let result_lr = unify_type(c.clone(), ta.clone(), tb.clone()).and_then(|tab| unify_type(c.clone(), tab, tc.clone()));
            let result_rl = unify_type(c.clone(), tb, tc).and_then(|tbc| unify_type(c, ta, tbc));
            match result_lr {
                Ok(tlr) =>
                    prop_assert_eq!(Ok(tlr), result_rl),
                Err(_) =>
                    match result_rl {
                        Ok(_) => panic!("Err, Ok"),
                        Err(_) => ()
                    }
            }
        }
    }
}
