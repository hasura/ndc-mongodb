//! Field types for relational expressions, and the literal conversion they drive.
//!
//! The GraphQL query path types each comparison value by the field it targets and converts it with
//! `json_to_bson`, so a string compared with an `ObjectId` field becomes an ObjectId. Relational
//! queries carry Arrow types instead, which have no ObjectId or UUID (both are advertised as
//! strings), so here a literal is typed by the field it is compared with.

use configuration::MongoScalarType;
use mongodb::bson::Bson;
use mongodb_support::{BsonScalarType, EXTENDED_JSON_TYPE_NAME};
use ndc_models::{self as ndc, CastType, Relation, RelationalExpression, RelationalLiteral};
use serde_json::Value;

use crate::{
    mongo_query_plan::{MongoConfiguration, Type},
    query::serialization::json_to_bson,
};

use super::column_origin::trace_column_origin;

/// Resolves the BSON types of fields referenced by expressions over one relation.
#[derive(Clone, Copy, Default)]
pub struct FieldTypes<'a> {
    source: Option<(&'a MongoConfiguration, &'a Relation)>,
}

impl<'a> FieldTypes<'a> {
    /// `relation` is the input whose columns the expressions index into.
    pub fn new(config: Option<&'a MongoConfiguration>, relation: &'a Relation) -> Self {
        Self {
            source: config.map(|config| (config, relation)),
        }
    }

    /// The type of a column or nested field reference, traced back to its source collection.
    pub fn of(&self, expr: &RelationalExpression) -> Option<&'a ndc::Type> {
        let (config, relation) = self.source?;
        let (collection, path) = field_origin(relation, expr)?;
        lookup_field_type(config, &collection, &path)
    }
}

fn field_origin(relation: &Relation, expr: &RelationalExpression) -> Option<(String, String)> {
    match expr {
        RelationalExpression::Column { index } => {
            let origin = trace_column_origin(relation, *index);
            Some((origin.collection?, origin.original_path?))
        }
        RelationalExpression::GetField { column, field } => {
            let (collection, path) = field_origin(relation, column)?;
            Some((collection, format!("{path}.{field}")))
        }
        _ => None,
    }
}

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

/// The literal an operand evaluates to, looking through casts that leave it unchanged. The engine
/// wraps permission-filter values in `CAST(value AS <column type>)`, which would otherwise hide
/// the literal from the conversions below.
pub fn literal_operand(expr: &RelationalExpression) -> Option<&RelationalLiteral> {
    match expr {
        RelationalExpression::Literal { literal } => Some(literal),
        RelationalExpression::Cast { expr, as_type, .. }
        | RelationalExpression::TryCast { expr, as_type, .. } => {
            let literal = literal_operand(expr)?;
            is_identity_cast(literal, as_type).then_some(literal)
        }
        _ => None,
    }
}

fn is_identity_cast(literal: &RelationalLiteral, as_type: &CastType) -> bool {
    use RelationalLiteral as L;
    matches!(
        (literal, as_type),
        (L::Null, _)
            | (L::String { .. }, CastType::Utf8)
            | (L::Boolean { .. }, CastType::Boolean)
            | (L::Int8 { .. }, CastType::Int8)
            | (L::Int16 { .. }, CastType::Int16)
            | (L::Int32 { .. }, CastType::Int32)
            | (L::Int64 { .. }, CastType::Int64)
            | (L::Float32 { .. }, CastType::Float32)
            | (L::Float64 { .. }, CastType::Float64)
    )
}

/// A comparison between a field reference and a literal, with the field on the left.
pub struct FieldComparison<'e> {
    pub field: &'e RelationalExpression,
    pub operator: &'static str,
    pub literal: &'e RelationalLiteral,
}

/// Match `field <op> literal` or `literal <op> field`. The caller checks that `field` really is a
/// field reference.
pub fn field_comparison(predicate: &RelationalExpression) -> Option<FieldComparison<'_>> {
    use RelationalExpression as E;
    let (left, right, operator, flipped) = match predicate {
        E::Eq { left, right } => (left, right, "$eq", "$eq"),
        E::NotEq { left, right } => (left, right, "$ne", "$ne"),
        E::Lt { left, right } => (left, right, "$lt", "$gt"),
        E::LtEq { left, right } => (left, right, "$lte", "$gte"),
        E::Gt { left, right } => (left, right, "$gt", "$lt"),
        E::GtEq { left, right } => (left, right, "$gte", "$lte"),
        _ => return None,
    };
    match (literal_operand(left), literal_operand(right)) {
        (_, Some(literal)) => Some(FieldComparison {
            field: left,
            operator,
            literal,
        }),
        (Some(literal), None) => Some(FieldComparison {
            field: right,
            operator: flipped,
            literal,
        }),
        (None, None) => None,
    }
}

/// Match `field IN (literal, ...)`. A NULL in the list never matches in SQL but would match
/// missing fields in a query document, so such lists are left to `$expr`.
pub fn field_in_literals(
    predicate: &RelationalExpression,
) -> Option<(&RelationalExpression, Vec<&RelationalLiteral>)> {
    let RelationalExpression::In { expr, list } = predicate else {
        return None;
    };
    let literals = list
        .iter()
        .map(|item| literal_operand(item).filter(|l| !matches!(l, RelationalLiteral::Null)))
        .collect::<Option<Vec<_>>>()?;
    Some((expr, literals))
}

/// BSON for a literal compared with a field of `field_type`, for a query document. Returns `None`
/// for literals a query document can't express.
pub fn literal_to_bson_with_field_type(
    literal: &RelationalLiteral,
    field_type: Option<&ndc::Type>,
) -> Option<Bson> {
    match literal {
        RelationalLiteral::Boolean { value } => Some(Bson::Boolean(*value)),
        RelationalLiteral::String { value } => Some(string_to_bson_for_field(value, field_type)),
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

/// A string compared with a field of `field_type`, converted with `json_to_bson` as the GraphQL
/// path converts a comparison value of that type. Only strings are converted this way: other
/// literals already carry their type, and temporal ones arrive as integers `json_to_bson` would
/// reject. A string that doesn't convert (non-hex text against an ObjectId field) stays a string,
/// so the comparison matches nothing instead of failing the query.
pub fn string_to_bson_for_field(value: &str, field_type: Option<&ndc::Type>) -> Bson {
    let as_string = || Bson::String(value.to_owned());
    match field_type.map(comparison_value_type) {
        None | Some(Type::Scalar(MongoScalarType::Bson(BsonScalarType::String))) => as_string(),
        Some(value_type) => json_to_bson(&value_type, Value::String(value.to_owned()))
            .unwrap_or_else(|_| as_string()),
    }
}

/// The type of a scalar compared with a field. Arrays compare by element, as in the GraphQL path's
/// `into_array_element_type`.
fn comparison_value_type(field_type: &ndc::Type) -> Type {
    match field_type {
        ndc::Type::Nullable {
            underlying_type: inner,
        }
        | ndc::Type::Array {
            element_type: inner,
        } => comparison_value_type(inner),
        other => ndc_type_to_plan_type(other),
    }
}

/// Convert an NDC type into the internal query-plan type used by `json_to_bson`.
///
/// `RelationalLiteral` only expresses scalars and null, so object/predicate types (which cannot be
/// produced by a relational literal) are treated as Extended JSON.
pub fn ndc_type_to_plan_type(t: &ndc::Type) -> Type {
    match t {
        ndc::Type::Named { name } => {
            let name = name.to_string();
            if name == EXTENDED_JSON_TYPE_NAME {
                Type::Scalar(MongoScalarType::ExtendedJSON)
            } else {
                // Scalar type names in the NDC schema use graphql names (e.g. `ObjectId`,
                // `Int`); `from_bson_name` matches case-insensitively against the BSON names.
                match BsonScalarType::from_bson_name(&name) {
                    Ok(scalar_type) => Type::Scalar(MongoScalarType::Bson(scalar_type)),
                    // Object/collection type names cannot describe a scalar literal argument;
                    // treat as Extended JSON so `json_to_bson` handles it generically.
                    Err(_) => Type::Scalar(MongoScalarType::ExtendedJSON),
                }
            }
        }
        ndc::Type::Nullable { underlying_type } => {
            Type::Nullable(Box::new(ndc_type_to_plan_type(underlying_type)))
        }
        ndc::Type::Array { element_type } => {
            Type::ArrayOf(Box::new(ndc_type_to_plan_type(element_type)))
        }
        ndc::Type::Predicate { .. } => Type::Scalar(MongoScalarType::ExtendedJSON),
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
