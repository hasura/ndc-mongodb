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

#[derive(Debug)]
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

#[derive(Debug, Error)]
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
}

fn make_nullable(t: Type) -> Type {
    match t {
        Type::Nullable(t) => Type::Nullable(t),
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

/// The types of two `ObjectField`s.
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
pub fn unify_schema(schema_a: Schema, schema_b: Schema) -> TypeUnificationResult<Schema> {
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
    Ok(Schema {
        collections,
        object_types,
    })
}
