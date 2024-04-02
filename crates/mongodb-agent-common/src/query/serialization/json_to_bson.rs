use std::{collections::BTreeMap, str::FromStr};

use configuration::schema::{ObjectType, Type};
use itertools::Itertools as _;
use mongodb::bson::{self, Bson, Decimal128};
use mongodb_support::BsonScalarType;
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use time::{format_description::well_known::Iso8601, OffsetDateTime};

use super::json_formats;

#[derive(Debug, Error)]
pub enum JsonToBsonError {
    #[error("error converting \"{1}\" to type, \"{0:?}\"")]
    ConversionError(Type, Value),

    #[error("error converting \"{1}\" to type, \"{0:?}\": {2}")]
    ConversionErrorWithContext(Type, Value, #[source] anyhow::Error),

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
pub fn json_to_bson(
    expected_type: &Type,
    object_types: &BTreeMap<String, ObjectType>,
    value: Value,
) -> Result<Bson> {
    match expected_type {
        Type::Any => serde_json::from_value::<Bson>(value).map_err(JsonToBsonError::SerdeError),
        Type::Scalar(t) => json_to_bson_scalar(*t, value),
        Type::Object(object_type_name) => {
            let object_type = object_types
                .get(object_type_name)
                .ok_or_else(|| JsonToBsonError::UnknownObjectType(object_type_name.to_owned()))?;
            convert_object(object_type_name, object_type, object_types, value)
        }
        Type::ArrayOf(element_type) => convert_array(element_type, object_types, value),
        Type::Nullable(t) => convert_nullable(t, object_types, value),
    }
}

/// Works like json_to_bson, but only converts BSON scalar types.
pub fn json_to_bson_scalar(expected_type: BsonScalarType, value: Value) -> Result<Bson> {
    let result = match expected_type {
        BsonScalarType::Double => Bson::Double(deserialize(expected_type, value)?),
        BsonScalarType::Int => Bson::Int32(deserialize(expected_type, value)?),
        BsonScalarType::Long => Bson::Int64(deserialize(expected_type, value)?),
        BsonScalarType::Decimal => Bson::Decimal128(
            Decimal128::from_str(&from_string(expected_type, value.clone())?).map_err(|err| {
                JsonToBsonError::ConversionErrorWithContext(
                    Type::Scalar(expected_type),
                    value,
                    err.into(),
                )
            })?,
        ),
        BsonScalarType::String => Bson::String(deserialize(expected_type, value)?),
        BsonScalarType::Date => convert_date(&from_string(expected_type, value)?)?,
        BsonScalarType::Timestamp => {
            deserialize::<json_formats::Timestamp>(expected_type, value)?.into()
        }
        BsonScalarType::BinData => {
            deserialize::<json_formats::BinData>(expected_type, value)?.into()
        }
        BsonScalarType::ObjectId => Bson::ObjectId(deserialize(expected_type, value)?),
        BsonScalarType::Bool => match value {
            Value::Bool(b) => Bson::Boolean(b),
            _ => incompatible_scalar_type(BsonScalarType::Bool, value)?,
        },
        BsonScalarType::Null => match value {
            Value::Null => Bson::Null,
            _ => incompatible_scalar_type(BsonScalarType::Null, value)?,
        },
        BsonScalarType::Undefined => match value {
            Value::Null => Bson::Undefined,
            _ => incompatible_scalar_type(BsonScalarType::Undefined, value)?,
        },
        BsonScalarType::Regex => deserialize::<json_formats::Regex>(expected_type, value)?.into(),
        BsonScalarType::Javascript => Bson::JavaScriptCode(deserialize(expected_type, value)?),
        BsonScalarType::JavascriptWithScope => {
            deserialize::<json_formats::JavaScriptCodeWithScope>(expected_type, value)?.into()
        }
        BsonScalarType::MinKey => Bson::MinKey,
        BsonScalarType::MaxKey => Bson::MaxKey,
        BsonScalarType::Symbol => Bson::Symbol(deserialize(expected_type, value)?),
        // dbPointer is deprecated
        BsonScalarType::DbPointer => Err(JsonToBsonError::NotImplemented(expected_type))?,
    };
    Ok(result)
}

fn convert_array(
    element_type: &Type,
    object_types: &BTreeMap<String, ObjectType>,
    value: Value,
) -> Result<Bson> {
    let input_elements: Vec<Value> = serde_json::from_value(value)?;
    let bson_array = input_elements
        .into_iter()
        .map(|v| json_to_bson(element_type, object_types, v))
        .try_collect()?;
    Ok(Bson::Array(bson_array))
}

fn convert_object(
    object_type_name: &str,
    object_type: &ObjectType,
    object_types: &BTreeMap<String, ObjectType>,
    value: Value,
) -> Result<Bson> {
    let input_fields: BTreeMap<String, Value> = serde_json::from_value(value)?;
    let bson_doc: bson::Document = object_type
        .named_fields()
        .map(|field| {
            let input_field_value = input_fields.get(field.name).ok_or_else(|| {
                JsonToBsonError::MissingObjectField(
                    Type::Object(object_type_name.to_owned()),
                    field.name.to_owned(),
                )
            })?;
            Ok((
                field.name.to_owned(),
                json_to_bson(&field.value.r#type, object_types, input_field_value.clone())?,
            ))
        })
        .try_collect::<_, _, JsonToBsonError>()?;
    Ok(bson_doc.into())
}

fn convert_nullable(
    underlying_type: &Type,
    object_types: &BTreeMap<String, ObjectType>,
    value: Value,
) -> Result<Bson> {
    match value {
        Value::Null => Ok(Bson::Null),
        non_null_value => json_to_bson(underlying_type, object_types, non_null_value),
    }
}

fn convert_date(value: &str) -> Result<Bson> {
    let date = OffsetDateTime::parse(value, &Iso8601::DEFAULT).map_err(|err| {
        JsonToBsonError::ConversionErrorWithContext(
            Type::Scalar(BsonScalarType::Date),
            Value::String(value.to_owned()),
            err.into(),
        )
    })?;
    Ok(Bson::DateTime(bson::DateTime::from_system_time(
        date.into(),
    )))
}

fn deserialize<T>(expected_type: BsonScalarType, value: Value) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_value::<T>(value.clone()).map_err(|err| {
        JsonToBsonError::ConversionErrorWithContext(Type::Scalar(expected_type), value, err.into())
    })
}

fn from_string(expected_type: BsonScalarType, value: Value) -> Result<String> {
    match value {
        Value::String(s) => Ok(s),
        _ => Err(JsonToBsonError::IncompatibleBackingType {
            expected_type: Type::Scalar(expected_type),
            expected_backing_type: "String",
            value,
        }),
    }
}

fn incompatible_scalar_type<T>(expected_type: BsonScalarType, value: Value) -> Result<T> {
    Err(JsonToBsonError::IncompatibleType(
        Type::Scalar(expected_type),
        value,
    ))
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, str::FromStr};

    use configuration::schema::{ObjectField, ObjectType, Type};
    use mongodb::bson::{self, datetime::DateTimeBuilder, Bson};
    use mongodb_support::BsonScalarType;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    use super::json_to_bson;

    #[test]
    #[allow(clippy::approx_constant)]
    fn deserializes_specialized_scalar_types() -> anyhow::Result<()> {
        let object_type_name = "scalar_test".to_owned();
        let object_type = ObjectType {
            fields: BTreeMap::from([
                ObjectField::new("double", Type::Scalar(BsonScalarType::Double)),
                ObjectField::new("int", Type::Scalar(BsonScalarType::Int)),
                ObjectField::new("long", Type::Scalar(BsonScalarType::Long)),
                ObjectField::new("decimal", Type::Scalar(BsonScalarType::Decimal)),
                ObjectField::new("string", Type::Scalar(BsonScalarType::String)),
                ObjectField::new("date", Type::Scalar(BsonScalarType::Date)),
                ObjectField::new("timestamp", Type::Scalar(BsonScalarType::Timestamp)),
                ObjectField::new("binData", Type::Scalar(BsonScalarType::BinData)),
                ObjectField::new("objectId", Type::Scalar(BsonScalarType::ObjectId)),
                ObjectField::new("bool", Type::Scalar(BsonScalarType::Bool)),
                ObjectField::new("null", Type::Scalar(BsonScalarType::Null)),
                ObjectField::new("undefined", Type::Scalar(BsonScalarType::Undefined)),
                ObjectField::new("regex", Type::Scalar(BsonScalarType::Regex)),
                ObjectField::new("javascript", Type::Scalar(BsonScalarType::Javascript)),
                ObjectField::new(
                    "javascriptWithScope",
                    Type::Scalar(BsonScalarType::JavascriptWithScope),
                ),
                ObjectField::new("minKey", Type::Scalar(BsonScalarType::MinKey)),
                ObjectField::new("maxKey", Type::Scalar(BsonScalarType::MaxKey)),
                ObjectField::new("symbol", Type::Scalar(BsonScalarType::Symbol)),
            ]),
            description: Default::default(),
        };

        let input = json!({
            "double": 3.14159,
            "int": 3,
            "long": 3,
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

        let actual = json_to_bson(
            &Type::Object(object_type_name.clone()),
            &[(object_type_name.clone(), object_type)]
                .into_iter()
                .collect(),
            input,
        )?;
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
            &Type::ArrayOf(Box::new(Type::Scalar(BsonScalarType::ObjectId))),
            &Default::default(),
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
                BsonScalarType::ObjectId,
            ))))),
            &Default::default(),
            input,
        )?;
        assert_eq!(actual, expected);
        Ok(())
    }
}
