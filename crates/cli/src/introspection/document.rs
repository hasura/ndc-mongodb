use configuration::{
    schema::{Collection, ObjectField, ObjectType, Type},
    Schema,
};
use indexmap::IndexMap;
use mongodb::bson::{Bson, Document};
use mongodb_agent_common::interface_types::{MongoAgentError, MongoConfig};
use mongodb_support::{
    align::align_with_result,
    BsonScalarType::{self, *},
    BsonType,
};
use std::string::String;

pub fn schema_from_document(collection_name: &str, document: &Document) -> Schema {
    let (object_types, collection) = make_collection(collection_name, document);
    Schema {
        collections: vec![collection],
        object_types,
    }
}

fn make_collection(collection_name: &str, document: &Document) -> (Vec<ObjectType>, Collection) {
    let object_type_defs = make_object_type(collection_name, document);
    let collection_info = Collection {
        name: collection_name.to_string(),
        description: None,
        r#type: collection_name.to_string(),
    };

    (object_type_defs, collection_info)
}

fn make_object_type(object_type_name: &str, document: &Document) -> Vec<ObjectType> {
    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{object_type_name}_");
        let (object_type_defs, object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = document
            .iter()
            .map(|(field_name, field_value)| {
                make_object_fields(&type_prefix, field_name, field_value)
            })
            .unzip();
        (object_type_defs.concat(), object_fields)
    };

    let object_type = ObjectType {
        name: object_type_name.to_string(),
        description: None,
        fields: object_fields,
    };

    object_type_defs.push(object_type);
    object_type_defs
}

fn make_object_fields(
    type_prefix: &str,
    field_name: &str,
    field_value: &Bson,
) -> (Vec<ObjectType>, ObjectField) {
    let object_type_name = format!("{type_prefix}{field_name}");
    let (collected_otds, field_type) = make_field_type(&object_type_name, field_value);

    let object_field = ObjectField {
        name: field_name.to_owned(),
        description: None,
        r#type: Type::Nullable(Box::new(field_type)),
    };

    (collected_otds, object_field)
}

fn make_field_type(object_type_name: &str, field_value: &Bson) -> (Vec<ObjectType>, Type) {
    fn scalar(t: BsonScalarType) -> (Vec<ObjectType>, Type) {
        (vec![], Type::Scalar(t))
    }
    match field_value {
        Bson::Double(_) => scalar(Double),
        Bson::String(_) => scalar(String),
        Bson::Array(arr) => {
            // TODO: examine all elements of the array and take the union.
            let (collected_otds, element_type) = match arr.first() {
                Some(elem) => make_field_type(object_type_name, elem),
                None => scalar(Undefined),
            };
            (collected_otds, Type::ArrayOf(Box::new(element_type)))
        }
        Bson::Document(document) => {
            let collected_otds = make_object_type(object_type_name, document);
            (collected_otds, Type::Object(object_type_name.to_owned()))
        }
        Bson::Boolean(_) => scalar(Bool),
        Bson::Null => scalar(Null),
        Bson::RegularExpression(_) => scalar(Regex),
        Bson::JavaScriptCode(_) => scalar(Javascript),
        Bson::JavaScriptCodeWithScope(_) => scalar(JavascriptWithScope),
        Bson::Int32(_) => scalar(Int),
        Bson::Int64(_) => scalar(Long),
        Bson::Timestamp(_) => scalar(Timestamp),
        Bson::Binary(_) => scalar(BinData),
        Bson::ObjectId(_) => scalar(ObjectId),
        Bson::DateTime(_) => scalar(Date),
        Bson::Symbol(_) => scalar(Symbol),
        Bson::Decimal128(_) => scalar(Decimal),
        Bson::Undefined => scalar(Undefined),
        Bson::MaxKey => scalar(MaxKey),
        Bson::MinKey => scalar(MinKey),
        Bson::DbPointer(_) => scalar(DbPointer),
    }
}

pub enum TypeUnificationError {
    ScalarTypeMismatch(BsonScalarType, BsonScalarType),
    ObjectTypeMismatch(String, String),
    TypeKindMismatch(Type, Type),
}

fn unify_types(type_a: Type, type_b: Type) -> Result<Type, TypeUnificationError> {
    match (type_a, type_b) {
        // If one type is undefined, the union is the other type.
        // This is used as the base case when inferring array types from documents.
        (Type::Scalar(Undefined), type_b) => Ok(type_b),
        (type_a, Type::Scalar(Undefined)) => Ok(type_a),

        // Union of any type with Null is the Nullable version of that type
        (Type::Scalar(Null), type_b) => Ok(make_nullable(type_b)),
        (type_a, Type::Scalar(Null)) => Ok(make_nullable(type_a)),

        (Type::Scalar(scalar_a), Type::Scalar(scalar_b)) => {
            if scalar_a == scalar_b {
                Ok(Type::Scalar(scalar_a))
            } else {
                Err(TypeUnificationError::ScalarTypeMismatch(scalar_a, scalar_b))
            }
        }
        (Type::Object(object_a), Type::Object(object_b)) => {
            if object_a == object_b {
                Ok(Type::Object(object_a))
            } else {
                Err(TypeUnificationError::ObjectTypeMismatch(object_a, object_b))
            }
        }
        (Type::ArrayOf(elem_type_a), Type::ArrayOf(elem_type_b)) => {
            let elem_type = unify_types(*elem_type_a, *elem_type_b)?;
            Ok(Type::ArrayOf(Box::new(elem_type)))
        }
        (Type::Nullable(nullable_type_a), type_b) => {
            let result_type = unify_types(*nullable_type_a, type_b)?;
            Ok(make_nullable(result_type))
        }
        (type_a, Type::Nullable(nullable_type_b)) => {
            let result_type = unify_types(type_a, *nullable_type_b)?;
            Ok(make_nullable(result_type))
        }
        (type_a, type_b) => Err(TypeUnificationError::TypeKindMismatch(type_a, type_b)),
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

fn unify_object_types(
    object_type_a: ObjectType,
    object_type_b: ObjectType,
) -> Result<ObjectType, TypeUnificationError> {
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
        unify_object_fields,
    )?;

    Ok(ObjectType {
        name: object_type_a.name,
        fields: merged_field_map.into_values().collect(),
        description: object_type_a.description.or(object_type_b.description),
    })
}

fn unify_object_fields(
    object_field_a: ObjectField,
    object_field_b: ObjectField,
) -> Result<ObjectField, TypeUnificationError> {
    Ok(ObjectField {
        name: object_field_a.name,
        r#type: unify_types(object_field_a.r#type, object_field_b.r#type)?,
        description: object_field_a.description.or(object_field_b.description),
    })
}

fn unify(
    object_types_a: Vec<ObjectType>,
    object_types_b: Vec<ObjectType>,
) -> Result<Vec<ObjectType>, TypeUnificationError> {
    let type_map_a: IndexMap<String, ObjectType> = object_types_a
        .into_iter()
        .map(|t| (t.name.to_owned(), t))
        .collect();
    let type_map_b: IndexMap<String, ObjectType> = object_types_b
        .into_iter()
        .map(|t| (t.name.to_owned(), t))
        .collect();

    let merged_type_map = align_with_result(type_map_a, type_map_b, Ok, Ok, unify_object_types)?;

    Ok(merged_type_map.into_values().collect())
}
