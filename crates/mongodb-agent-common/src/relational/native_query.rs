//! Resolution and materialization of collection-representation native queries used as relational
//! tables (`Relation::From`).
//!
//! When a `Relation::From.collection` names a configured native query rather than a physical
//! MongoDB collection, we must:
//!
//! 1. reject function-representation native queries,
//! 2. validate and bind the supplied `From.arguments` against the declared argument types,
//! 3. interpolate the configured pipeline with those bound argument values, producing an
//!    immutable "source prefix" of stages, and
//! 4. determine the physical collection (the native query's `input_collection`) — or `None` for a
//!    database-level aggregation.
//!
//! This reuses the classic native-query machinery: `json_to_bson` for typed BSON conversion and
//! validation, and `interpolated_command` for `{{ argument }}` substitution.

use std::collections::BTreeMap;

use configuration::{
    native_query::{NativeQuery, NativeQueryRepresentation},
    MongoScalarType,
};
use mongodb::bson::Bson;
use mongodb_support::{aggregate::Stage, BsonScalarType, EXTENDED_JSON_TYPE_NAME};
use ndc_models::{self as ndc, ArgumentName, RelationalLiteral};
use serde_json::Value;

use crate::{
    mongo_query_plan::{MongoConfiguration, Type},
    procedure::interpolated_command,
    query::serialization::json_to_bson,
};

use super::RelationalError;

/// The immutable source prefix produced by materializing a native query, along with the physical
/// collection (if any) the aggregation should run against.
pub struct MaterializedNativeQuery {
    /// Interpolated native-query pipeline stages, in order. These must remain first in the final
    /// pipeline so that Atlas stages such as `$search`/`$searchMeta`/`$vectorSearch` stay at
    /// stage zero.
    pub prefix_stages: Vec<Stage>,
    /// Physical collection to run against (`input_collection`), or `None` for a database-level
    /// aggregation.
    pub target_collection: Option<String>,
}

/// Look up a configured native query by the name used in a `Relation::From`.
pub fn lookup_native_query<'a>(
    config: &'a MongoConfiguration,
    collection: &ndc::CollectionName,
) -> Option<&'a NativeQuery> {
    config.native_queries().get(collection)
}

/// Materialize a collection-representation native query into its source prefix.
pub fn materialize_native_query(
    config: &MongoConfiguration,
    name: &ndc::CollectionName,
    native_query: &NativeQuery,
    arguments: &BTreeMap<ArgumentName, RelationalLiteral>,
) -> Result<MaterializedNativeQuery, RelationalError> {
    // A function-representation native query does not describe a list of documents, so it cannot
    // stand in as a relational table.
    if native_query.representation == NativeQueryRepresentation::Function {
        return Err(RelationalError::FunctionRepresentationNotSupported(
            name.to_string(),
        ));
    }

    let bson_arguments = bind_arguments(config, name, arguments)?;

    let prefix_stages = native_query
        .pipeline
        .iter()
        .map(|document| {
            interpolated_command(document, &bson_arguments)
                .map(Stage::Other)
                .map_err(|error| RelationalError::InterpolationError {
                    native_query: name.to_string(),
                    message: error.to_string(),
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let target_collection = native_query
        .input_collection
        .as_ref()
        .map(|collection| collection.to_string());

    Ok(MaterializedNativeQuery {
        prefix_stages,
        target_collection,
    })
}

/// Validate the supplied arguments against the native query's declared arguments and convert each
/// to BSON using the declared type.
///
/// Declared argument types are read from the native query's generated `CollectionInfo`. Reports
/// unknown, missing, null-into-non-nullable, and type-invalid arguments as errors.
fn bind_arguments(
    config: &MongoConfiguration,
    name: &ndc::CollectionName,
    provided: &BTreeMap<ArgumentName, RelationalLiteral>,
) -> Result<BTreeMap<ArgumentName, Bson>, RelationalError> {
    let empty = BTreeMap::new();
    let declared = config
        .0
        .collections
        .get(name)
        .map(|collection_info| &collection_info.arguments)
        .unwrap_or(&empty);

    // Reject any argument the native query does not declare.
    for argument_name in provided.keys() {
        if !declared.contains_key(argument_name) {
            return Err(RelationalError::UnknownArgument {
                native_query: name.to_string(),
                argument: argument_name.to_string(),
            });
        }
    }

    // Bind every declared argument; each must be supplied.
    declared
        .iter()
        .map(|(argument_name, argument_info)| {
            let literal =
                provided
                    .get(argument_name)
                    .ok_or_else(|| RelationalError::MissingArgument {
                        native_query: name.to_string(),
                        argument: argument_name.to_string(),
                    })?;

            let expected_type = ndc_type_to_plan_type(&argument_info.argument_type);
            let value = coerce_literal_json(relational_literal_to_json(literal), &expected_type);
            let bson = json_to_bson(&expected_type, value).map_err(|error| {
                RelationalError::ArgumentBindingError {
                    native_query: name.to_string(),
                    argument: argument_name.to_string(),
                    message: error.to_string(),
                }
            })?;

            Ok((argument_name.clone(), bson))
        })
        .collect()
}

/// Adjust the JSON representation of a literal so it matches the backing type `json_to_bson`
/// expects for the declared scalar type.
///
/// `json_to_bson` binds `Long` and `Decimal` from JSON strings (arbitrary-precision safe), but a
/// relational integer literal arrives as a JSON number. Stringify numbers destined for those
/// types so the natural relational literal binds cleanly. All other types accept their natural
/// representation.
fn coerce_literal_json(value: Value, expected_type: &Type) -> Value {
    match scalar_type_of(expected_type) {
        Some(BsonScalarType::Long | BsonScalarType::Decimal) if value.is_number() => {
            Value::String(value.to_string())
        }
        _ => value,
    }
}

/// The scalar type underlying a (possibly nullable) type, if it is a BSON scalar.
fn scalar_type_of(t: &Type) -> Option<BsonScalarType> {
    match t {
        Type::Scalar(MongoScalarType::Bson(scalar_type)) => Some(*scalar_type),
        Type::Nullable(inner) => scalar_type_of(inner),
        _ => None,
    }
}

/// Convert a declared NDC argument type into the internal query-plan type used by `json_to_bson`.
///
/// `RelationalLiteral` only expresses scalars and null, so object/predicate types (which cannot be
/// produced by a relational literal) are treated as Extended JSON.
fn ndc_type_to_plan_type(t: &ndc::Type) -> Type {
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

/// Convert a `RelationalLiteral` to the plain JSON representation expected by `json_to_bson`.
///
/// `json_to_bson` performs the type-directed coercion (e.g. string → `ObjectId`, string → `Date`,
/// numeric widening) and the accompanying validation, so here we only need faithful JSON values.
fn relational_literal_to_json(literal: &RelationalLiteral) -> Value {
    use RelationalLiteral as L;
    match literal {
        L::Null => Value::Null,
        L::Boolean { value } => Value::Bool(*value),
        L::String { value } => Value::String(value.clone()),
        L::Int8 { value } => Value::from(*value),
        L::Int16 { value } => Value::from(*value),
        L::Int32 { value } => Value::from(*value),
        L::Int64 { value } => Value::from(*value),
        L::UInt8 { value } => Value::from(*value),
        L::UInt16 { value } => Value::from(*value),
        L::UInt32 { value } => Value::from(*value),
        L::UInt64 { value } => Value::from(*value),
        L::Float32 { value: ndc::Float32(v) } => {
            serde_json::Number::from_f64(f64::from(*v)).map_or(Value::Null, Value::Number)
        }
        L::Float64 { value: ndc::Float64(v) } => {
            serde_json::Number::from_f64(*v).map_or(Value::Null, Value::Number)
        }
        // Temporal, decimal, duration, and interval literals are represented as their underlying
        // integer/string values. `json_to_bson` will accept these where the declared argument type
        // can parse them (e.g. `Long`, `Decimal`), and reject them otherwise.
        L::Decimal128 { value, .. } => Value::String(value.to_string()),
        L::Decimal256 { value, .. } => Value::String(value.clone()),
        L::Date32 { value } => Value::from(*value),
        L::Date64 { value } => Value::from(*value),
        L::Time32Second { value } | L::Time32Millisecond { value } => Value::from(*value),
        L::Time64Microsecond { value } | L::Time64Nanosecond { value } => Value::from(*value),
        L::TimestampSecond { value }
        | L::TimestampMillisecond { value }
        | L::TimestampMicrosecond { value }
        | L::TimestampNanosecond { value } => Value::from(*value),
        L::DurationSecond { value }
        | L::DurationMillisecond { value }
        | L::DurationMicrosecond { value }
        | L::DurationNanosecond { value } => Value::from(*value),
        L::Interval {
            months,
            days,
            nanoseconds,
        } => serde_json::json!({
            "months": months,
            "days": days,
            "nanoseconds": nanoseconds,
        }),
    }
}
