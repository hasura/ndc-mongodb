use itertools::Itertools as _;
use mongodb::bson::{self, Bson};
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

    #[error("error converting value to JSON: {0}")]
    Serde(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, BsonToJsonError>;

/// Converts BSON values to JSON.
///
/// The BSON library already has a `Serialize` impl that can convert to JSON. But that
/// implementation emits Extended JSON which includes inline type tags in JSON output to
/// disambiguate types on the BSON side. We don't want those tags because we communicate type
/// information out of band.
pub fn bson_to_json(value: Bson) -> Result<Value> {
    match value {
        Bson::Null => Ok(Value::Null),
        Bson::Undefined => Ok(Value::Null),
        Bson::MaxKey => Ok(Value::Object(Default::default())),
        Bson::MinKey => Ok(Value::Object(Default::default())),
        Bson::Boolean(b) => Ok(Value::Bool(b)),
        Bson::Int32(n) => Ok(Value::Number(<i32 as Into<_>>::into(n))),
        Bson::Int64(n) => Ok(Value::Number(<i64 as Into<_>>::into(n))),
        Bson::Double(n) => Ok(Value::Number(
            Number::from_f64(n).ok_or(BsonToJsonError::DoubleConversion(n))?,
        )),
        Bson::Decimal128(n) => Ok(Value::String(n.to_string())),
        Bson::String(s) => Ok(Value::String(s)),
        Bson::Symbol(s) => Ok(Value::String(s)),
        Bson::DateTime(date) => convert_date(date),
        Bson::JavaScriptCode(s) => Ok(Value::String(s)),
        Bson::JavaScriptCodeWithScope(v) => convert_code(v),
        Bson::RegularExpression(regex) => Ok(to_value::<json_formats::Regex>(regex.into())?),
        Bson::Timestamp(v) => Ok(to_value::<json_formats::Timestamp>(v.into())?),
        Bson::Binary(b) => Ok(to_value::<json_formats::BinData>(b.into())?),
        Bson::ObjectId(oid) => Ok(to_value(oid)?),
        Bson::DbPointer(v) => Ok(to_value(v)?),
        Bson::Array(vs) => Ok(Value::Array(
            vs.into_iter().map(bson_to_json).try_collect()?,
        )),
        Bson::Document(fields) => Ok(Value::Object(
            fields
                .into_iter()
                .map(|(name, value)| bson_to_json(value).map(|bson| (name, bson)))
                .try_collect()?,
        )),
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
