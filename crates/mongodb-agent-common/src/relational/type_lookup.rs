use mongodb::bson::{oid::ObjectId, Bson};
use mongodb_support::BsonScalarType;
use ndc_models::{self as ndc, RelationalLiteral};

use crate::mongo_query_plan::MongoConfiguration;

pub fn lookup_field_type<'a>(
    config: &'a MongoConfiguration,
    collection: &str,
    field_path: &str,
) -> Option<&'a ndc::Type> {
    let collection_info = config.0.collections.get(collection)?;
    let object_type = config.0.object_types.get(&collection_info.collection_type)?;
    let path_segments = field_path
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();

    lookup_field_type_in_object(config, object_type, &path_segments)
}

pub fn literal_to_bson_with_field_type(
    literal: &RelationalLiteral,
    field_type: Option<&ndc::Type>,
) -> Option<Bson> {
    match literal {
        RelationalLiteral::Boolean { value } => Some(Bson::Boolean(*value)),
        RelationalLiteral::String { value } => {
            if is_object_id_type(field_type) {
                ObjectId::parse_str(value)
                    .map(Bson::ObjectId)
                    .ok()
                    .or_else(|| Some(Bson::String(value.clone())))
            } else {
                Some(Bson::String(value.clone()))
            }
        }
        RelationalLiteral::Int8 { value } => Some(Bson::Int32(i32::from(*value))),
        RelationalLiteral::Int16 { value } => Some(Bson::Int32(i32::from(*value))),
        RelationalLiteral::Int32 { value } => Some(Bson::Int32(*value)),
        RelationalLiteral::Int64 { value } => Some(Bson::Int64(*value)),
        RelationalLiteral::Float32 { value: ndc::Float32(v) } => Some(Bson::Double(f64::from(*v))),
        RelationalLiteral::Float64 { value: ndc::Float64(v) } => Some(Bson::Double(*v)),
        RelationalLiteral::Null => Some(Bson::Null),
        _ => None,
    }
}

fn lookup_field_type_in_object<'a>(
    config: &'a MongoConfiguration,
    object_type: &'a ndc::ObjectType,
    path_segments: &[&str],
) -> Option<&'a ndc::Type> {
    let (segment, rest) = path_segments.split_first()?;
    let field = object_type.fields.get(*segment)?;

    if rest.is_empty() {
        return Some(&field.r#type);
    }

    let nested_object = lookup_object_type_for_type(config, &field.r#type)?;
    lookup_field_type_in_object(config, nested_object, rest)
}

fn lookup_object_type_for_type<'a>(
    config: &'a MongoConfiguration,
    field_type: &'a ndc::Type,
) -> Option<&'a ndc::ObjectType> {
    match field_type {
        ndc::Type::Named { name } => config.0.object_types.get(name),
        ndc::Type::Nullable { underlying_type } => {
            lookup_object_type_for_type(config, underlying_type)
        }
        _ => None,
    }
}

fn is_object_id_type(field_type: Option<&ndc::Type>) -> bool {
    match field_type {
        Some(ndc::Type::Named { name }) => {
            name.to_string() == BsonScalarType::ObjectId.graphql_name()
        }
        Some(ndc::Type::Nullable { underlying_type }) => is_object_id_type(Some(underlying_type)),
        _ => false,
    }
}
