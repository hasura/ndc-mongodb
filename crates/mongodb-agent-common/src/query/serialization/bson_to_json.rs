use configuration::MongoScalarType;
use itertools::Itertools as _;
use mongodb::bson::{self, Bson};
use mongodb_support::{BsonScalarType, ExtendedJsonMode};
use serde_json::{to_value, Number, Value};
use thiserror::Error;
use time::{format_description::well_known::Iso8601, OffsetDateTime};

use crate::mongo_query_plan::{ObjectType, Type};

use super::{is_nullable, json_formats};

#[derive(Debug, Error)]
pub enum BsonToJsonError {
    #[error("error reading date-time value from BSON: {0}")]
    DateConversion(String),

    #[error("error converting 64-bit floating point number from BSON to JSON: {0}")]
    DoubleConversion(f64),

    #[error("input object of type {0:?} is missing a field, \"{1}\"")]
    MissingObjectField(Type, String),

    #[error("error converting value to JSON: {0}")]
    Serde(#[from] serde_json::Error),

    // TODO: It would be great if we could capture a path into the larger BSON value here
    #[error("expected a value of type {0:?}, but got {1}")]
    TypeMismatch(Type, Bson),

    #[error("unknown object type, \"{0}\"")]
    UnknownObjectType(String),
}

type Result<T> = std::result::Result<T, BsonToJsonError>;

/// Converts BSON values to JSON.
///
/// The BSON library already has a `Serialize` impl that can convert to JSON. But that
/// implementation emits Extended JSON which includes inline type tags in JSON output to
/// disambiguate types on the BSON side. We don't want those tags because we communicate type
/// information out of band. That is except for the `Type::ExtendedJSON` type where we do want to emit
/// Extended JSON because we don't have out-of-band information in that case.
pub fn bson_to_json(mode: ExtendedJsonMode, expected_type: &Type, value: Bson) -> Result<Value> {
    match expected_type {
        Type::Scalar(configuration::MongoScalarType::ExtendedJSON) => Ok(mode.into_extjson(value)),
        Type::Scalar(MongoScalarType::Bson(scalar_type)) => {
            bson_scalar_to_json(mode, *scalar_type, value)
        }
        Type::Object(object_type) => convert_object(mode, object_type, value),
        Type::ArrayOf(element_type) => convert_array(mode, element_type, value),
        Type::Nullable(t) => convert_nullable(mode, t, value),
    }
}

// Converts values while checking against the expected type. But there are a couple of cases where
// we do implicit conversion where the BSON types have indistinguishable JSON representations, and
// values can be converted back to BSON without loss of meaning.
fn bson_scalar_to_json(
    mode: ExtendedJsonMode,
    expected_type: BsonScalarType,
    value: Bson,
) -> Result<Value> {
    match (expected_type, value) {
        (BsonScalarType::Null | BsonScalarType::Undefined, Bson::Null | Bson::Undefined) => {
            Ok(Value::Null)
        }
        (BsonScalarType::MinKey, Bson::MinKey) => Ok(Value::Object(Default::default())),
        (BsonScalarType::MaxKey, Bson::MaxKey) => Ok(Value::Object(Default::default())),
        (BsonScalarType::Bool, Bson::Boolean(b)) => Ok(Value::Bool(b)),
        (BsonScalarType::Double, v) => convert_small_number(expected_type, v),
        (BsonScalarType::Int, v) => convert_small_number(expected_type, v),
        (BsonScalarType::Long, Bson::Int64(n)) => Ok(Value::String(n.to_string())),
        (BsonScalarType::Decimal, Bson::Decimal128(n)) => Ok(Value::String(n.to_string())),
        (BsonScalarType::String, Bson::String(s)) => Ok(Value::String(s)),
        (BsonScalarType::Symbol, Bson::Symbol(s)) => Ok(Value::String(s)),
        (BsonScalarType::Date, Bson::DateTime(date)) => convert_date(date),
        (BsonScalarType::Javascript, Bson::JavaScriptCode(s)) => Ok(Value::String(s)),
        (BsonScalarType::JavascriptWithScope, Bson::JavaScriptCodeWithScope(v)) => {
            convert_code(mode, v)
        }
        (BsonScalarType::Regex, Bson::RegularExpression(regex)) => {
            Ok(to_value::<json_formats::Regex>(regex.into())?)
        }
        (BsonScalarType::Timestamp, Bson::Timestamp(v)) => {
            Ok(to_value::<json_formats::Timestamp>(v.into())?)
        }
        (BsonScalarType::BinData, Bson::Binary(b)) => {
            Ok(to_value::<json_formats::BinData>(b.into())?)
        }
        (BsonScalarType::ObjectId, Bson::ObjectId(oid)) => Ok(Value::String(oid.to_hex())),
        (BsonScalarType::DbPointer, v) => Ok(mode.into_extjson(v)),
        (_, v) => Err(BsonToJsonError::TypeMismatch(
            Type::Scalar(MongoScalarType::Bson(expected_type)),
            v,
        )),
    }
}

fn convert_array(mode: ExtendedJsonMode, element_type: &Type, value: Bson) -> Result<Value> {
    let values = match value {
        Bson::Array(values) => Ok(values),
        _ => Err(BsonToJsonError::TypeMismatch(
            Type::ArrayOf(Box::new(element_type.clone())),
            value,
        )),
    }?;
    let json_array = values
        .into_iter()
        .map(|value| bson_to_json(mode, element_type, value))
        .try_collect()?;
    Ok(Value::Array(json_array))
}

fn convert_object(mode: ExtendedJsonMode, object_type: &ObjectType, value: Bson) -> Result<Value> {
    let input_doc = match value {
        Bson::Document(fields) => Ok(fields),
        _ => Err(BsonToJsonError::TypeMismatch(
            Type::Object(object_type.to_owned()),
            value,
        )),
    }?;
    let json_obj: serde_json::Map<String, Value> = object_type
        .named_fields()
        .filter_map(|field| {
            let field_value_result =
                get_object_field_value(object_type, field, &input_doc).transpose()?;
            Some((field, field_value_result))
        })
        .map(|((field_name, field_type), field_value_result)| {
            Ok((
                field_name.to_owned(),
                bson_to_json(mode, field_type, field_value_result?)?,
            ))
        })
        .try_collect::<_, _, BsonToJsonError>()?;
    Ok(Value::Object(json_obj))
}

// Gets value for the appropriate key from the input object. Returns `Ok(None)` if the value is
// missing, and the field is nullable. Returns `Err` if the value is missing and the field is *not*
// nullable.
fn get_object_field_value(
    object_type: &ObjectType,
    (field_name, field_type): (&str, &Type),
    doc: &bson::Document,
) -> Result<Option<Bson>> {
    let value = doc.get(field_name);
    if value.is_none() && is_nullable(field_type) {
        return Ok(None);
    }
    Ok(Some(value.cloned().ok_or_else(|| {
        BsonToJsonError::MissingObjectField(
            Type::Object(object_type.clone()),
            field_name.to_owned(),
        )
    })?))
}

fn convert_nullable(mode: ExtendedJsonMode, underlying_type: &Type, value: Bson) -> Result<Value> {
    match value {
        Bson::Null => Ok(Value::Null),
        non_null_value => bson_to_json(mode, underlying_type, non_null_value),
    }
}

// Use custom conversion instead of type in json_formats to get extjson output
fn convert_code(mode: ExtendedJsonMode, v: bson::JavaScriptCodeWithScope) -> Result<Value> {
    Ok(Value::Object(
        [
            ("$code".to_owned(), Value::String(v.code)),
            (
                "$scope".to_owned(),
                mode.into_extjson(Into::<Bson>::into(v.scope)),
            ),
        ]
        .into_iter()
        .collect(),
    ))
}

// We could convert directly from bson::DateTime to OffsetDateTime if the bson feature `time-0_3`
// were set. Unfortunately it is difficult for us to set that feature since we get bson via
// mongodb.
fn convert_date(date: bson::DateTime) -> Result<Value> {
    let system_time = date.to_system_time();
    let offset_date: OffsetDateTime = system_time.into();
    let string = offset_date
        .format(&Iso8601::DEFAULT)
        .map_err(|err| BsonToJsonError::DateConversion(err.to_string()))?;
    Ok(Value::String(string))
}

// We can mix up doubles and 32-bit ints because they both map to JSON numbers, we don't lose
// precision, and the carry approximately the same meaning when converted back to BSON with the
// reversed type.
fn convert_small_number(expected_type: BsonScalarType, value: Bson) -> Result<Value> {
    match value {
        Bson::Double(n) => Ok(Value::Number(
            Number::from_f64(n).ok_or(BsonToJsonError::DoubleConversion(n))?,
        )),
        Bson::Int32(n) => Ok(Value::Number(n.into())),
        _ => Err(BsonToJsonError::TypeMismatch(
            Type::Scalar(MongoScalarType::Bson(expected_type)),
            value,
        )),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn serializes_object_id_to_string() -> anyhow::Result<()> {
        let expected_string = "573a1390f29313caabcd446f";
        let json = bson_to_json(
            ExtendedJsonMode::Canonical,
            &Type::Scalar(MongoScalarType::Bson(BsonScalarType::ObjectId)),
            Bson::ObjectId(FromStr::from_str(expected_string)?),
        )?;
        assert_eq!(json, Value::String(expected_string.to_owned()));
        Ok(())
    }

    #[test]
    fn serializes_document_with_missing_nullable_field() -> anyhow::Result<()> {
        let expected_type = Type::Object(ObjectType {
            name: Some("test_object".into()),
            fields: [(
                "field".to_owned(),
                Type::Nullable(Box::new(Type::Scalar(MongoScalarType::Bson(
                    BsonScalarType::String,
                )))),
            )]
            .into(),
        });
        let value = bson::doc! {};
        let actual = bson_to_json(ExtendedJsonMode::Canonical, &expected_type, value.into())?;
        assert_eq!(actual, json!({}));
        Ok(())
    }
}
