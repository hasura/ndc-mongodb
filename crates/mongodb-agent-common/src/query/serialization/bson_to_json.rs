use itertools::Itertools as _;
use mongodb::bson::{self, Bson};
use serde_json::{Number, Value};
use thiserror::Error;
use time::{format_description::well_known::Iso8601, OffsetDateTime};

#[derive(Debug, Error)]
pub enum BsonToJsonError {
    #[error("error reading date-time value from BSON: {0}")]
    DateConversion(String),

    #[error("error converting 64-bit floating point number from BSON to JSON: {0}")]
    DoubleConversion(f64),
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
        Bson::RegularExpression(regex) => convert_regex(regex),
        Bson::Timestamp(v) => convert_timestamp(v),
        Bson::Binary(_) => todo!(),
        Bson::ObjectId(_) => todo!(),
        Bson::DbPointer(_) => todo!(),
        Bson::Array(vs) => Ok(Value::Array(
            vs.into_iter().map(bson_to_json).try_collect()?,
        )),
        Bson::Document(_) => todo!(),
    }
}

fn convert_code(v: bson::JavaScriptCodeWithScope) -> Result<Value> {
    Ok(Value::Object(
        [
            ("$code".to_owned(), Value::String(v.code)),
            (
                "$scope".to_owned(),
                serde_json::to_value(v.scope).expect("serializing JavaScriptCodeWithScope.scope"),
            ),
        ]
        .into_iter()
        .collect(),
    ))
}

fn convert_date(date: bson::DateTime) -> Result<Value> {
    let offset_date = OffsetDateTime::from_unix_timestamp(date.timestamp_millis())
        .map_err(|err| BsonToJsonError::DateConversion(err.to_string()))?;
    Ok(Value::String(
        offset_date
            .format(&Iso8601::DEFAULT)
            .map_err(|err| BsonToJsonError::DateConversion(err.to_string()))?,
    ))
}

fn convert_regex(regex: bson::Regex) -> Result<Value> {
    Ok(Value::Object(
        [
            ("pattern".to_owned(), Value::String(regex.pattern)),
            ("options".to_owned(), Value::String(regex.options)),
        ]
        .into_iter()
        .collect(),
    ))
}

fn convert_timestamp(timestamp: bson::Timestamp) -> Result<Value> {
    Ok(Value::Object(
        [
            (
                "t".to_owned(),
                Value::Number(<u32 as Into<_>>::into(timestamp.time)),
            ),
            (
                "i".to_owned(),
                Value::Number(<u32 as Into<_>>::into(timestamp.increment)),
            ),
        ]
        .into_iter()
        .collect(),
    ))
}
