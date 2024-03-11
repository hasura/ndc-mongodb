use dc_api_types::GraphQlType;
use enum_iterator::{all, Sequence};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::error::Error;

// According to the MongoDB documentation this is the list of BSON types.
// https://www.mongodb.com/docs/manual/reference/operator/query/type/#std-label-document-type-available-types
//
// - "double"
// - "string"
// - "object"
// - "array"
// - "binData"
// - "undefined"
// - "objectId"
// - "bool"
// - "date"
// - "null"
// - "regex"
// - "dbPointer"
// - "javascript"
// - "symbol"
// - "javascriptWithScope"
// - "int"
// - "timestamp"
// - "long"
// - "decimal"
// - "minKey"
// - "maxKey"
//
// This list does not include "number" which is an alias for "double", "int", "long", or "decimal"

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BsonType {
    Object,
    Array,
    Scalar(BsonScalarType),
}

impl<'de> Deserialize<'de> for BsonType {
    /// bson_type may be a string, or an array containing a single string
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let type_name = if let Some(s) = value.as_str() {
            Ok(s)
        } else if let Some(a) = value.as_array() {
            if a.len() == 1 {
                if let Some(s) = a[0].as_str() {
                    Ok(s)
                } else {
                    Err(serde::de::Error::custom(
                        "expected bsonType array to contain a string",
                    ))
                }
            } else {
                Err(serde::de::Error::custom(
                    "expected bsonType array to contain exactly one string",
                ))
            }
        } else {
            Err(serde::de::Error::custom(format!(
        "found bsonType that is neither a string, nor an array containing a single string: {value}")))
        }?;

        match type_name {
            "object" => Ok(BsonType::Object),
            "array" => Ok(BsonType::Array),
            name => {
                let scalar_type = BsonScalarType::from_bson_name(name).map_err(|_| {
                    serde::de::Error::custom(format!("unknown BSON scalar type, {name}"))
                })?;
                Ok(BsonType::Scalar(scalar_type))
            }
        }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Sequence, Deserialize, JsonSchema)]
#[serde(try_from = "BsonType", rename_all = "camelCase")]
pub enum BsonScalarType {
    // numeric
    Double,
    Decimal,
    Int,
    Long,

    // text
    String,

    // date/time
    Date,
    Timestamp,

    // other
    BinData,
    ObjectId,
    Bool,
    Null,
    Regex,
    Javascript,
    JavascriptWithScope,
    MinKey,
    MaxKey,

    // other other
    Undefined,
    DbPointer,
    Symbol,
}

use BsonScalarType as S;

impl BsonScalarType {
    pub fn bson_name(self) -> &'static str {
        match self {
            S::Double => "double",
            S::Decimal => "decimal",
            S::Int => "int",
            S::Long => "long",
            S::String => "string",
            S::Date => "date",
            S::Timestamp => "timestamp",
            S::BinData => "binData",
            S::ObjectId => "objectId",
            S::Bool => "bool",
            S::Null => "null",
            S::Regex => "regex",
            S::Javascript => "javascript",
            S::JavascriptWithScope => "javascriptWithScope",
            S::MinKey => "minKey",
            S::MaxKey => "maxKey",
            S::Undefined => "undefined",
            S::DbPointer => "dbPointer",
            S::Symbol => "symbol",
        }
    }

    pub fn graphql_name(self) -> String {
        match self.graphql_type() {
            Some(gql_type) => gql_type.to_string(),
            None => capitalize(self.bson_name()),
        }
    }

    pub fn graphql_type(self) -> Option<GraphQlType> {
        match self {
            S::Double => Some(GraphQlType::Float),
            S::String => Some(GraphQlType::String),
            S::Int => Some(GraphQlType::Int),
            S::Bool => Some(GraphQlType::Boolean),
            _ => None,
        }
    }

    pub fn from_bson_name(name: &str) -> Result<Self, Error> {
        // "number" is an alias for "double", "int", or "long". Assume the most general-ish case of
        // "double"
        if name == "number" {
            return Ok(S::Double);
        }
        let scalar_type = all::<BsonScalarType>().find(|s| s.bson_name() == name);
        scalar_type.ok_or_else(|| Error::UnknownScalarType(name.to_owned()))
    }
}

impl TryFrom<BsonType> for BsonScalarType {
    type Error = Error;

    fn try_from(value: BsonType) -> Result<Self, Self::Error> {
        match value {
            BsonType::Scalar(scalar_type) => Ok(scalar_type),
            _ => Err(Error::ExpectedScalarType(value)),
        }
    }
}

/// Capitalizes the first character in s.
fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use crate::BsonScalarType;

    use super::BsonType;

    #[test]
    fn parses_object_type_from_array() -> Result<(), anyhow::Error> {
        let t: BsonType = serde_json::from_str(r#"["object"]"#)?;
        assert_eq!(t, BsonType::Object);
        Ok(())
    }

    #[test]
    fn parses_scalar_type_from_array() -> Result<(), anyhow::Error> {
        let t: BsonType = serde_json::from_str(r#"["double"]"#)?;
        assert_eq!(t, BsonType::Scalar(BsonScalarType::Double));
        Ok(())
    }
}
