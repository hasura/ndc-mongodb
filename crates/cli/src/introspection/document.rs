use configuration::{
    schema::{Collection, ObjectField, ObjectType, Type},
    Schema,
};
use futures_util::TryStreamExt;
use indexmap::IndexMap;
use mongodb::bson::{doc, Bson, Document};
use mongodb_agent_common::interface_types::MongoConfig;
use mongodb_support::{
    align::align_with_result,
    BsonScalarType::{self, *},
};
use std::{
    fmt::{self, Display},
    string::String,
};
use thiserror::Error;

// Sample from all collections in the database
pub async fn sample_schema_from_db(
    sample_size: u32,
    config: &MongoConfig,
) -> anyhow::Result<Schema> {
    let mut schema = Schema {
        collections: vec![],
        object_types: vec![],
    };
    let db = config.client.database(&config.database);
    let mut collections_cursor = db.list_collections(None, None).await?;

    while let Some(collection_spec) = collections_cursor.try_next().await? {
        let collection_name = collection_spec.name;
        let collection_schema =
            sample_schema_from_collection(&collection_name, sample_size, config).await?;
        schema = unify_schema(schema, collection_schema)?;
    }
    Ok(schema)
}

pub async fn sample_schema_from_collection(
    collection_name: &str,
    sample_size: u32,
    config: &MongoConfig,
) -> anyhow::Result<Schema> {
    let db = config.client.database(&config.database);
    let options = None;
    let mut cursor = db
        .collection::<Document>(collection_name)
        .aggregate(vec![doc! {"$sample": { "size": sample_size }}], options)
        .await?;
    let mut collected_object_types = vec![];
    while let Some(document) = cursor.try_next().await? {
        let object_types = make_object_type(collection_name, &document)?;
        collected_object_types = unify_object_types(collected_object_types, object_types)?;
    }
    let collection_info = Collection {
        name: collection_name.to_string(),
        description: None,
        r#type: collection_name.to_string(),
    };

    Ok(Schema {
        collections: vec![collection_info],
        object_types: collected_object_types,
    })
}

fn make_object_type(
    object_type_name: &str,
    document: &Document,
) -> TypeUnificationResult<Vec<ObjectType>> {
    let (mut object_type_defs, object_fields) = {
        let type_prefix = format!("{object_type_name}_");
        let (object_type_defs, object_fields): (Vec<Vec<ObjectType>>, Vec<ObjectField>) = document
            .iter()
            .map(|(field_name, field_value)| {
                make_object_field(&type_prefix, field_name, field_value)
            })
            .collect::<TypeUnificationResult<Vec<(Vec<ObjectType>, ObjectField)>>>()?
            .into_iter()
            .unzip();
        (object_type_defs.concat(), object_fields)
    };

    let object_type = ObjectType {
        name: object_type_name.to_string(),
        description: None,
        fields: object_fields,
    };

    object_type_defs.push(object_type);
    Ok(object_type_defs)
}

fn make_object_field(
    type_prefix: &str,
    field_name: &str,
    field_value: &Bson,
) -> TypeUnificationResult<(Vec<ObjectType>, ObjectField)> {
    let object_type_name = format!("{type_prefix}{field_name}");
    let (collected_otds, field_type) = make_field_type(&object_type_name, field_name, field_value)?;

    let object_field = ObjectField {
        name: field_name.to_owned(),
        description: None,
        r#type: field_type,
    };

    Ok((collected_otds, object_field))
}

fn make_field_type(
    object_type_name: &str,
    field_name: &str,
    field_value: &Bson,
) -> TypeUnificationResult<(Vec<ObjectType>, Type)> {
    fn scalar(t: BsonScalarType) -> TypeUnificationResult<(Vec<ObjectType>, Type)> {
        Ok((vec![], Type::Scalar(t)))
    }
    match field_value {
        Bson::Double(_) => scalar(Double),
        Bson::String(_) => scalar(String),
        Bson::Array(arr) => {
            // Examine all elements of the array and take the union of the resulting types.
            let mut collected_otds = vec![];
            let mut result_type = Type::Scalar(Undefined);
            for elem in arr {
                let (elem_collected_otds, elem_type) =
                    make_field_type(object_type_name, field_name, elem)?;
                collected_otds = unify_object_types(collected_otds, elem_collected_otds)?;
                let context = TypeUnificationContext::new(object_type_name, field_name);
                result_type = unify_type(context, result_type, elem_type)?;
            }
            Ok((collected_otds, Type::ArrayOf(Box::new(result_type))))
        }
        Bson::Document(document) => {
            let collected_otds = make_object_type(object_type_name, document)?;
            Ok((collected_otds, Type::Object(object_type_name.to_owned())))
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

#[derive(Debug)]
pub struct TypeUnificationContext {
    object_type_name: String,
    field_name: String,
}

impl TypeUnificationContext {
    fn new(object_type_name: &str, field_name: &str) -> Self {
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
    ScalarTypeMismatch(TypeUnificationContext, BsonScalarType, BsonScalarType),
    ObjectTypeMismatch(String, String),
    TypeKindMismatch(Type, Type),
}

impl Display for TypeUnificationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScalarTypeMismatch(context, scalar_a, scalar_b) => write!(
                f,
                "Scalar type mismatch {} {} at {}",
                scalar_a.bson_name(),
                scalar_b.bson_name(),
                context
            ),
            Self::ObjectTypeMismatch(object_a, object_b) => {
                write!(f, "Object type mismatch {} {}", object_a, object_b)
            }
            Self::TypeKindMismatch(type_a, type_b) => {
                write!(f, "Object type mismatch {:?} {:?}", type_a, type_b)
            }
        }
    }
}

type TypeUnificationResult<T> = Result<T, TypeUnificationError>;

fn unify_type(
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

        (Type::Scalar(scalar_a), Type::Scalar(scalar_b)) => {
            if scalar_a == scalar_b {
                Ok(Type::Scalar(scalar_a))
            } else {
                Err(TypeUnificationError::ScalarTypeMismatch(
                    context, scalar_a, scalar_b,
                ))
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
            let elem_type = unify_type(context, *elem_type_a, *elem_type_b)?;
            Ok(Type::ArrayOf(Box::new(elem_type)))
        }
        (Type::Nullable(nullable_type_a), type_b) => {
            let result_type = unify_type(context, *nullable_type_a, type_b)?;
            Ok(make_nullable(result_type))
        }
        (type_a, Type::Nullable(nullable_type_b)) => {
            let result_type = unify_type(context, type_a, *nullable_type_b)?;
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

fn unify_object_types(
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

// Unify two schemas. Assumes that the schemas describe mutually exclusive sets of collections.
fn unify_schema(schema_a: Schema, schema_b: Schema) -> TypeUnificationResult<Schema> {
    let collections = schema_a
        .collections
        .into_iter()
        .chain(schema_b.collections.into_iter())
        .collect();
    let object_types = schema_a
        .object_types
        .into_iter()
        .chain(schema_b.object_types.into_iter())
        .collect();
    Ok(Schema {
        collections,
        object_types,
    })
}
