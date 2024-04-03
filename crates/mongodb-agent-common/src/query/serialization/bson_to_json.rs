use std::collections::BTreeMap;

use configuration::{
    schema::{ObjectField, ObjectType, Type},
    WithNameRef,
};
use itertools::Itertools as _;
use mongodb::bson::{self, Bson};
use mongodb_support::BsonScalarType;
use serde_json::{to_value, Number, Value};
use thiserror::Error;
use time::{format_description::well_known::Iso8601, OffsetDateTime};

use super::json_formats;

#[derive(Debug, Error)]
pub enum BsonToJsonError {
    #[error("error reading date-time value from BSON: {0}")]
    DateConversion(String),

    #[error("error converting 64-bit floating point number from BSON to JSON: {0}")]
    DoubleConversion(f64),

    #[error("input object of type \"{0:?}\" is missing a field, \"{1}\"")]
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
/// information out of band. That is except for the `Type::Any` type where we do want to emit
/// Extended JSON because we don't have out-of-band information in that case.
pub fn bson_to_json(
    expected_type: &Type,
    object_types: &BTreeMap<String, ObjectType>,
    value: Bson,
) -> Result<Value> {
    match expected_type {
        Type::Any => Ok(value.into_canonical_extjson()),
        Type::Scalar(scalar_type) => bson_scalar_to_json(*scalar_type, value),
        Type::Object(object_type_name) => {
            let object_type = object_types
                .get(object_type_name)
                .ok_or_else(|| BsonToJsonError::UnknownObjectType(object_type_name.to_owned()))?;
            convert_object(object_type_name, object_type, object_types, value)
        }
        Type::ArrayOf(element_type) => convert_array(element_type, object_types, value),
        Type::Nullable(t) => convert_nullable(t, object_types, value),
    }
}

// Converts values while checking against the expected type. But there are a couple of cases where
// we do implicit conversion where the BSON types have indistinguishable JSON representations, and
// values can be converted back to BSON without loss of meaning.
fn bson_scalar_to_json(expected_type: BsonScalarType, value: Bson) -> Result<Value> {
    match (expected_type, value) {
        (BsonScalarType::Null, Bson::Null) => Ok(Value::Null),
        (BsonScalarType::Undefined, Bson::Undefined) => Ok(Value::Null),
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
        (BsonScalarType::JavascriptWithScope, Bson::JavaScriptCodeWithScope(v)) => convert_code(v),
        (BsonScalarType::Regex, Bson::RegularExpression(regex)) => {
            Ok(to_value::<json_formats::Regex>(regex.into())?)
        }
        (BsonScalarType::Timestamp, Bson::Timestamp(v)) => {
            Ok(to_value::<json_formats::Timestamp>(v.into())?)
        }
        (BsonScalarType::BinData, Bson::Binary(b)) => {
            Ok(to_value::<json_formats::BinData>(b.into())?)
        }
        (BsonScalarType::ObjectId, Bson::ObjectId(oid)) => Ok(to_value(oid)?),
        (BsonScalarType::DbPointer, v) => Ok(v.into_canonical_extjson()),
        (_, v) => Err(BsonToJsonError::TypeMismatch(
            Type::Scalar(expected_type),
            v,
        )),
    }
}

fn convert_array(
    element_type: &Type,
    object_types: &BTreeMap<String, ObjectType>,
    value: Bson,
) -> Result<Value> {
    let values = match value {
        Bson::Array(values) => Ok(values),
        _ => Err(BsonToJsonError::TypeMismatch(
            Type::ArrayOf(Box::new(element_type.clone())),
            value,
        )),
    }?;
    let json_array = values
        .into_iter()
        .map(|value| bson_to_json(element_type, object_types, value))
        .try_collect()?;
    Ok(Value::Array(json_array))
}

fn convert_object(
    object_type_name: &str,
    object_type: &ObjectType,
    object_types: &BTreeMap<String, ObjectType>,
    value: Bson,
) -> Result<Value> {
    let input_doc = match value {
        Bson::Document(fields) => Ok(fields),
        _ => Err(BsonToJsonError::TypeMismatch(
            Type::Object(object_type_name.to_owned()),
            value,
        )),
    }?;
    let json_obj: serde_json::Map<String, Value> = object_type
        .named_fields()
        .map(|field| {
            let input_field_value =
                get_object_field_value(object_type_name, field.clone(), &input_doc)?;
            Ok((
                field.name.to_owned(),
                bson_to_json(&field.value.r#type, object_types, input_field_value.clone())?,
            ))
        })
        .try_collect::<_, _, BsonToJsonError>()?;
    Ok(Value::Object(json_obj))
}

fn get_object_field_value(
    object_type_name: &str,
    field: WithNameRef<'_, ObjectField>,
    doc: &bson::Document,
) -> Result<Bson> {
    let value = doc.get(field.name);
    if value.is_none() && field.value.r#type.is_nullable() {
        return Ok(Bson::Null);
    }
    value.cloned().ok_or_else(|| {
        BsonToJsonError::MissingObjectField(
            Type::Object(object_type_name.to_owned()),
            field.name.to_owned(),
        )
    })
}

fn convert_nullable(
    underlying_type: &Type,
    object_types: &BTreeMap<String, ObjectType>,
    value: Bson,
) -> Result<Value> {
    match value {
        Bson::Null => Ok(Value::Null),
        non_null_value => bson_to_json(underlying_type, object_types, non_null_value),
    }
}

// Use custom conversion instead of type in json_formats to get canonical extjson output
fn convert_code(v: bson::JavaScriptCodeWithScope) -> Result<Value> {
    Ok(Value::Object(
        [
            ("$code".to_owned(), Value::String(v.code)),
            (
                "$scope".to_owned(),
                Into::<Bson>::into(v.scope).into_canonical_extjson(),
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
            Type::Scalar(expected_type),
            value,
        )),
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::*;

    #[test]
    fn serializes_document_with_missing_nullable_field() -> anyhow::Result<()> {
        let expected_type = Type::Object("test_object".to_owned());
        let object_types = [(
            "test_object".to_owned(),
            ObjectType {
                fields: [(
                    "field".to_owned(),
                    ObjectField {
                        r#type: Type::Nullable(Box::new(Type::Scalar(BsonScalarType::String))),
                        description: None,
                    },
                )]
                .into(),
                description: None,
            },
        )]
        .into();
        let value = bson::doc! {};
        let actual = bson_to_json(&expected_type, &object_types, value.into())?;
        assert_eq!(actual, json!({ "field": null }));
        Ok(())
    }
}
