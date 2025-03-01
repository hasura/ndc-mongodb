use std::{collections::BTreeMap, num::ParseIntError, str::FromStr};

use configuration::MongoScalarType;
use itertools::Itertools as _;
use mongodb::bson::{self, Bson, Decimal128};
use mongodb_support::BsonScalarType;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use time::{format_description::well_known::Iso8601, OffsetDateTime};

use crate::mongo_query_plan::{ObjectType, Type};

use super::{helpers::is_nullable, json_formats};

#[derive(Debug, Error)]
pub enum JsonToBsonError {
    #[error("error converting \"{1}\" to type, \"{0:?}\"")]
    ConversionError(Type, Value),

    #[error("error converting \"{1}\" to type, \"{0:?}\": {2}")]
    ConversionErrorWithContext(Type, Value, #[source] anyhow::Error),

    #[error("error parsing \"{0}\" as a date. Date values should be in ISO 8601 format with a time component, like `2016-01-01T00:00Z`. Underlying error: {1}")]
    DateConversionErrorWithContext(Value, #[source] anyhow::Error),

    #[error("cannot use value, \"{0:?}\", in position of type, \"{1:?}\"")]
    IncompatibleType(Type, Value),

    #[error("input with BSON type {expected_type:?} should be encoded in GraphQL as {expected_backing_type}, but got: {value}")]
    IncompatibleBackingType {
        expected_type: Type,
        expected_backing_type: &'static str,
        value: Value,
    },

    #[error("input object of type \"{0:?}\" is missing a field, \"{1}\"")]
    MissingObjectField(Type, String),

    #[error("inputs of type {0} are not implemented")]
    NotImplemented(BsonScalarType),

    #[error("could not parse 64-bit integer input, {0}: {1}")]
    ParseInt(String, #[source] ParseIntError),

    #[error("error deserializing input: {0}")]
    SerdeError(#[from] serde_json::Error),

    #[error("unknown object type, \"{0}\"")]
    UnknownObjectType(String),
}

type Result<T> = std::result::Result<T, JsonToBsonError>;

/// Converts JSON input to BSON according to an expected BSON type.
///
/// The BSON library already has a `Deserialize` impl that can convert from JSON. But that
/// implementation cannot take advantage of the type information that we have available. Instead it
/// uses Extended JSON which uses tags in JSON data to distinguish BSON types.
pub fn json_to_bson(expected_type: &Type, value: Value) -> Result<Bson> {
    match expected_type {
        Type::Scalar(MongoScalarType::ExtendedJSON) => {
            serde_json::from_value::<Bson>(value).map_err(JsonToBsonError::SerdeError)
        }
        Type::Scalar(MongoScalarType::Bson(t)) => json_to_bson_scalar(*t, value),
        Type::Object(object_type) => convert_object(object_type, value),
        Type::ArrayOf(element_type) => convert_array(element_type, value),
        Type::Nullable(t) => convert_nullable(t, value),
    }
}

/// Works like json_to_bson, but only converts BSON scalar types.
pub fn json_to_bson_scalar(expected_type: BsonScalarType, value: Value) -> Result<Bson> {
    use BsonScalarType as S;
    let result = match expected_type {
        S::Double => Bson::Double(deserialize(expected_type, value)?),
        S::Int => Bson::Int32(deserialize(expected_type, value)?),
        S::Long => convert_long(&from_string(expected_type, value)?)?,
        S::Decimal => Bson::Decimal128(
            Decimal128::from_str(&from_string(expected_type, value.clone())?).map_err(|err| {
                JsonToBsonError::ConversionErrorWithContext(
                    Type::Scalar(MongoScalarType::Bson(expected_type)),
                    value,
                    err.into(),
                )
            })?,
        ),
        S::String => Bson::String(deserialize(expected_type, value)?),
        S::Date => convert_date(&from_string(expected_type, value)?)?,
        S::Timestamp => deserialize::<json_formats::Timestamp>(expected_type, value)?.into(),
        S::BinData => deserialize::<json_formats::BinData>(expected_type, value)?.into(),
        S::UUID => convert_uuid(&from_string(expected_type, value)?)?,
        S::ObjectId => Bson::ObjectId(deserialize(expected_type, value)?),
        S::Bool => match value {
            Value::Bool(b) => Bson::Boolean(b),
            _ => incompatible_scalar_type(S::Bool, value)?,
        },
        S::Null => match value {
            Value::Null => Bson::Null,
            _ => incompatible_scalar_type(S::Null, value)?,
        },
        S::Undefined => match value {
            Value::Null => Bson::Undefined,
            _ => incompatible_scalar_type(S::Undefined, value)?,
        },
        S::Regex => {
            deserialize::<json_formats::Either<json_formats::Regex, String>>(expected_type, value)?
                .into_left()
                .into()
        }
        S::Javascript => Bson::JavaScriptCode(deserialize(expected_type, value)?),
        S::JavascriptWithScope => {
            deserialize::<json_formats::JavaScriptCodeWithScope>(expected_type, value)?.into()
        }
        S::MinKey => Bson::MinKey,
        S::MaxKey => Bson::MaxKey,
        S::Symbol => Bson::Symbol(deserialize(expected_type, value)?),
        // dbPointer is deprecated
        S::DbPointer => Err(JsonToBsonError::NotImplemented(expected_type))?,
    };
    Ok(result)
}

fn convert_array(element_type: &Type, value: Value) -> Result<Bson> {
    let input_elements: Vec<Value> = serde_json::from_value(value)?;
    let bson_array = input_elements
        .into_iter()
        .map(|v| json_to_bson(element_type, v))
        .try_collect()?;
    Ok(Bson::Array(bson_array))
}

fn convert_object(object_type: &ObjectType, value: Value) -> Result<Bson> {
    let input_fields: BTreeMap<String, Value> = serde_json::from_value(value)?;
    let bson_doc: bson::Document = object_type
        .named_fields()
        .filter_map(|(name, field_type)| {
            let field_value_result =
                get_object_field_value(object_type, name, field_type, &input_fields).transpose()?;
            Some((name, field_type, field_value_result))
        })
        .map(|(name, field_type, field_value_result)| {
            Ok((
                name.to_string(),
                json_to_bson(field_type, field_value_result?)?,
            ))
        })
        .try_collect::<_, _, JsonToBsonError>()?;
    Ok(bson_doc.into())
}

// Gets value for the appropriate key from the input object. Returns `Ok(None)` if the value is
// missing, and the field is nullable. Returns `Err` if the value is missing and the field is *not*
// nullable.
fn get_object_field_value(
    object_type: &ObjectType,
    field_name: &ndc_models::FieldName,
    field_type: &Type,
    object: &BTreeMap<String, Value>,
) -> Result<Option<Value>> {
    let value = object.get(field_name.as_str());
    if value.is_none() && is_nullable(field_type) {
        return Ok(None);
    }
    Ok(Some(value.cloned().ok_or_else(|| {
        JsonToBsonError::MissingObjectField(
            Type::Object(object_type.clone()),
            field_name.to_string(),
        )
    })?))
}

fn convert_nullable(underlying_type: &Type, value: Value) -> Result<Bson> {
    match value {
        Value::Null => Ok(Bson::Null),
        non_null_value => json_to_bson(underlying_type, non_null_value),
    }
}

fn convert_date(value: &str) -> Result<Bson> {
    let date = OffsetDateTime::parse(value, &Iso8601::PARSING).map_err(|err| {
        JsonToBsonError::DateConversionErrorWithContext(Value::String(value.to_owned()), err.into())
    })?;
    Ok(Bson::DateTime(bson::DateTime::from_system_time(
        date.into(),
    )))
}

fn convert_long(value: &str) -> Result<Bson> {
    let n: i64 = value
        .parse()
        .map_err(|err| JsonToBsonError::ParseInt(value.to_owned(), err))?;
    Ok(Bson::Int64(n))
}

fn convert_uuid(value: &str) -> Result<Bson> {
    let uuid = bson::Uuid::parse_str(value).map_err(|err| {
        JsonToBsonError::ConversionErrorWithContext(
            Type::Scalar(MongoScalarType::Bson(BsonScalarType::UUID)),
            value.into(),
            err.into(),
        )
    })?;
    Ok(bson::binary::Binary::from_uuid(uuid).into())
}

fn deserialize<T>(expected_type: BsonScalarType, value: Value) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value::<T>(value.clone()).map_err(|err| {
        JsonToBsonError::ConversionErrorWithContext(
            Type::Scalar(MongoScalarType::Bson(expected_type)),
            value,
            err.into(),
        )
    })
}

fn from_string(expected_type: BsonScalarType, value: Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s),
        _ => Err(JsonToBsonError::IncompatibleBackingType {
            expected_type: Type::Scalar(MongoScalarType::Bson(expected_type)),
            expected_backing_type: "String",
            value,
        }),
    }
}

fn incompatible_scalar_type<T>(expected_type: BsonScalarType, value: Value) -> Result<T> {
    Err(JsonToBsonError::IncompatibleType(
        Type::Scalar(MongoScalarType::Bson(expected_type)),
        value,
    ))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use configuration::MongoScalarType;
    use mongodb::bson::{self, bson, datetime::DateTimeBuilder, Bson};
    use mongodb_support::BsonScalarType;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use crate::mongo_query_plan::{ObjectType, Type};

    use super::json_to_bson;

    use BsonScalarType as S;

    #[test]
    #[allow(clippy::approx_constant)]
    fn deserializes_specialized_scalar_types() -> anyhow::Result<()> {
        let object_type = ObjectType::new([
            ("double", Type::scalar(S::Double)),
            ("int", Type::scalar(S::Int)),
            ("long", Type::scalar(S::Long)),
            ("decimal", Type::scalar(S::Decimal)),
            ("string", Type::scalar(S::String)),
            ("date", Type::scalar(S::Date)),
            ("timestamp", Type::scalar(S::Timestamp)),
            ("binData", Type::scalar(S::BinData)),
            ("objectId", Type::scalar(S::ObjectId)),
            ("bool", Type::scalar(S::Bool)),
            ("null", Type::scalar(S::Null)),
            ("undefined", Type::scalar(S::Undefined)),
            ("regex", Type::scalar(S::Regex)),
            ("javascript", Type::scalar(S::Javascript)),
            ("javascriptWithScope", Type::scalar(S::JavascriptWithScope)),
            ("minKey", Type::scalar(S::MinKey)),
            ("maxKey", Type::scalar(S::MaxKey)),
            ("symbol", Type::scalar(S::Symbol)),
        ])
        .named("scalar_test");

        let input = json!({
            "double": 3.14159,
            "int": 3,
            "long": "3",
            "decimal": "3.14159",
            "string": "hello",
            "date": "2024-03-22T00:59:01Z",
            "timestamp": { "t": 1565545664, "i": 1 },
            "binData": {
                "base64": "EEEBEIEIERA=",
                "subType": "00"
            },
            "objectId": "e7c8f79873814cbae1f8d84c",
            "bool": true,
            "null": null,
            "undefined": null,
            "regex": { "pattern": "^fo+$", "options": "i" },
            "javascript": "console.log('hello, world!')",
            "javascriptWithScope": {
                "$code": "console.log('hello, ', name)",
                "$scope": { "name": "you!" },
            },
            "minKey": {},
            "maxKey": {},
            "symbol": "a_symbol",
        });

        let expected = bson::doc! {
            "double": Bson::Double(3.14159),
            "int": Bson::Int32(3),
            "long": Bson::Int64(3),
            "decimal": Bson::Decimal128(bson::Decimal128::from_str("3.14159")?),
            "string": Bson::String("hello".to_owned()),
            "date": Bson::DateTime(DateTimeBuilder::default().year(2024).month(3).day(22).hour(0).minute(59).second(1).build()?),
            "timestamp": Bson::Timestamp(bson::Timestamp { time: 1565545664, increment: 1 }),
            "binData": Bson::Binary(bson::Binary {
                bytes: vec![0x10, 0x41, 0x01, 0x10, 0x81, 0x08, 0x11, 0x10],
                subtype: bson::spec::BinarySubtype::Generic,
            }),
            "objectId": Bson::ObjectId(FromStr::from_str("e7c8f79873814cbae1f8d84c")?),
            "bool": Bson::Boolean(true),
            "null": Bson::Null,
            "undefined": Bson::Undefined,
            "regex": Bson::RegularExpression(bson::Regex { pattern: "^fo+$".to_owned(), options: "i".to_owned() }),
            "javascript": Bson::JavaScriptCode("console.log('hello, world!')".to_owned()),
            "javascriptWithScope": Bson::JavaScriptCodeWithScope(bson::JavaScriptCodeWithScope {
                code: "console.log('hello, ', name)".to_owned(),
                scope: bson::doc! { "name": "you!" },
            }),
            "minKey": Bson::MinKey,
            "maxKey": Bson::MaxKey,
            "symbol": Bson::Symbol("a_symbol".to_owned()),
        };

        let actual = json_to_bson(&Type::Object(object_type), input)?;
        assert_eq!(actual, expected.into());
        Ok(())
    }

    #[test]
    fn deserializes_arrays() -> anyhow::Result<()> {
        let input = json!([
            "e7c8f79873814cbae1f8d84c",
            "76a3317b46f1eea7fae4f643",
            "fae1840a2b85872385c67de5",
        ]);
        let expected = Bson::Array(vec![
            Bson::ObjectId(FromStr::from_str("e7c8f79873814cbae1f8d84c")?),
            Bson::ObjectId(FromStr::from_str("76a3317b46f1eea7fae4f643")?),
            Bson::ObjectId(FromStr::from_str("fae1840a2b85872385c67de5")?),
        ]);
        let actual = json_to_bson(
            &Type::ArrayOf(Box::new(Type::Scalar(MongoScalarType::Bson(
                BsonScalarType::ObjectId,
            )))),
            input,
        )?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn deserializes_nullable_values() -> anyhow::Result<()> {
        let input = json!(["e7c8f79873814cbae1f8d84c", null, "fae1840a2b85872385c67de5",]);
        let expected = Bson::Array(vec![
            Bson::ObjectId(FromStr::from_str("e7c8f79873814cbae1f8d84c")?),
            Bson::Null,
            Bson::ObjectId(FromStr::from_str("fae1840a2b85872385c67de5")?),
        ]);
        let actual = json_to_bson(
            &Type::ArrayOf(Box::new(Type::Nullable(Box::new(Type::Scalar(
                MongoScalarType::Bson(BsonScalarType::ObjectId),
            ))))),
            input,
        )?;
        assert_eq!(actual, expected);
        Ok(())
    }

    #[test]
    fn deserializes_object_with_missing_nullable_field() -> anyhow::Result<()> {
        let expected_type = Type::named_object(
            "test_object",
            [(
                "field",
                Type::nullable(Type::scalar(BsonScalarType::String)),
            )],
        );
        let value = json!({});
        let actual = json_to_bson(&expected_type, value)?;
        assert_eq!(actual, bson!({}));
        Ok(())
    }

    #[test]
    fn converts_string_input_to_date() -> anyhow::Result<()> {
        let input = json!("2016-01-01T00:00Z");
        let actual = json_to_bson(
            &Type::Scalar(MongoScalarType::Bson(BsonScalarType::Date)),
            input,
        )?;
        let expected = Bson::DateTime(bson::DateTime::from_millis(1_451_606_400_000));
        assert_eq!(actual, expected);
        Ok(())
    }
}
